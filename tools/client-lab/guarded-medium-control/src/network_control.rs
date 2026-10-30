//! Closed authored-worker adapter. Actual execution requires reviewed source,
//! CI and artifact admission outside this module. The high peer owns native evidence;
//! low never fabricates an event from a receipt or a filter count.
use crate::{
    control as common,
    ipc::Pipe,
    network_probe::{self, Listener},
    network_protocol as net,
    process::{self, Handle, Image, Peer},
    Prepared, Receipt, Result, Scenario,
};
use metallic_external_guard_fixture::{
    kernel::{self, Operation, Record, Status},
    Probe,
};
use std::{
    fs::File,
    io::{Seek, SeekFrom, Write},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use windows_sys::Win32::{
    System::{LibraryLoader::*, Threading::*},
    UI::{Shell::*, WindowsAndMessaging::SW_HIDE},
};

fn utc() -> Result<u64> {
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "network_clock_unavailable")?;
    time.as_secs()
        .checked_add(11644473600)
        .and_then(|s| s.checked_mul(10_000_000))
        .and_then(|s| s.checked_add(time.subsec_nanos() as u64 / 100))
        .ok_or("network_clock_bound")
}
fn budget(start: Instant, startup: bool) -> Result<()> {
    let elapsed = start.elapsed();
    if elapsed >= Duration::from_secs(90) || (startup && elapsed >= Duration::from_secs(30)) {
        Err("network_batch_deadline")
    } else {
        Ok(())
    }
}
fn stamp(value: crate::protocol::Stamp) -> net::Stamp {
    net::Stamp {
        pid: value.pid,
        created: value.created,
    }
}
fn legacy(value: net::Stamp) -> crate::protocol::Stamp {
    crate::protocol::Stamp {
        pid: value.pid,
        created: value.created,
    }
}
fn probe(value: Probe) -> net::Probe {
    match value {
        Probe::Tcp4 => net::Probe::Tcp4,
        Probe::Tcp6 => net::Probe::Tcp6,
        Probe::Udp4 => net::Probe::Udp4,
        Probe::Udp6 => net::Probe::Udp6,
    }
}
fn control(nonce: [u8; 16], kind: net::Kind, seq: u32) -> net::Frame {
    net::Frame {
        kind,
        nonce,
        seq,
        phase: None,
        probe: None,
        root: None,
        raw_exit: 0,
        udp: None,
    }
}
fn step(nonce: [u8; 16], kind: net::Kind, seq: u32, p: Probe, root: net::Root) -> net::Frame {
    net::Frame {
        kind,
        nonce,
        seq,
        phase: Some(net::Phase::Blocked),
        probe: Some(probe(p)),
        root: Some(root),
        raw_exit: 0,
        udp: None,
    }
}
fn send(pipe: &Pipe, value: net::Frame) -> Result<()> {
    pipe.send_network_bytes(value.encode()?)
}
fn receive(pipe: &Pipe) -> Result<net::Frame> {
    net::Frame::decode(&pipe.receive_network_bytes()?)
}
fn expect(actual: net::Frame, expected: net::Frame) -> Result<()> {
    if actual == expected {
        Ok(())
    } else {
        Err("network_frame_binding_refused")
    }
}
fn output() -> Result<(File, Vec<Handle>)> {
    let (dir, locks) = process::private_directory()?;
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(dir.join("network-batch.json"))
        .map_err(|_| "network_output_create_failed")?;
    file.write_all(b"{\"ok\":false,\"status\":\"pending\",\"executionReady\":false}")
        .and_then(|_| file.sync_all())
        .map_err(|_| "network_output_write_failed")?;
    Ok((file, locks))
}
fn report(
    file: &mut File,
    result: &Result<()>,
    high: bool,
    release_attempted: bool,
    released: bool,
) -> Result<()> {
    let text=match result{
        Ok(())if high=>"{\"ok\":true,\"scope\":\"authored_worker_guard_batch_only\",\"blocked\":4,\"retainedTargetsExited\":4,\"cleanupVerified\":true,\"targetNetworkVerified\":true,\"originalClientTested\":false,\"executionReady\":false}".to_string(),
        Ok(())=>"{\"ok\":true,\"scope\":\"authored_medium_network_only\",\"beforePositive\":4,\"blocked\":4,\"afterPositive\":4,\"cleanupVerified\":true,\"targetNetworkVerified\":true,\"originalClientTested\":false,\"executionReady\":false}".to_string(),
        Err(reason)if high=>format!("{{\"ok\":false,\"error\":\"{reason}\",\"guard\":true,\"releaseAttempted\":{release_attempted},\"releaseCommitted\":{released},\"cleanupVerified\":false,\"targetNetworkVerified\":false,\"originalClientTested\":false,\"executionReady\":false}}"),
        // Without Released, low cannot know whether a failed transport followed
        // a partial/committed high-side release. Report acknowledgement only.
        Err(reason)=>format!("{{\"ok\":false,\"error\":\"{reason}\",\"guard\":false,\"releaseAcknowledged\":{released},\"cleanupVerified\":false,\"targetNetworkVerified\":false,\"originalClientTested\":false,\"executionReady\":false}}"),
    };
    file.seek(SeekFrom::Start(0))
        .and_then(|_| file.write_all(text.as_bytes()))
        .and_then(|_| file.set_len(text.len() as u64))
        .and_then(|_| file.sync_all())
        .map_err(|_| "network_output_write_failed")
}
fn naturally_finished(receipt: &Receipt) -> Result<i32> {
    if !receipt.resumed
        || receipt.timed_out
        || receipt.before_cleanup.is_none()
        || receipt.before_cleanup != receipt.exit_code
        || !receipt.job_empty
        || receipt.total_processes != 1
        || !receipt.stdout.eof
        || !receipt.stderr.eof
        || receipt.stderr.bytes != 0
    {
        return Err("network_owned_cleanup_or_exit_unverified");
    }
    Ok(receipt.before_cleanup.unwrap() as i32)
}
fn metadata(receipt: &Receipt, p: Probe, port: u16) -> Result<Option<Record>> {
    if p.tcp() {
        if receipt.stdout.bytes != 0 {
            return Err("network_tcp_output_refused");
        }
        return Ok(None);
    }
    let bytes = receipt
        .stdout
        .metadata
        .as_ref()
        .ok_or("network_udp_metadata_missing")?;
    Record::decode(bytes, p, port)
        .map(Some)
        .ok_or("network_udp_metadata_refused")
}
fn wire_record(record: Record) -> net::UdpRecord {
    net::UdpRecord {
        family: record.family,
        operation: record.operation as u8,
        sent: record.sent,
        source_port: record.source_port,
        destination_port: record.destination_port,
    }
}
fn kernel_record(record: net::UdpRecord) -> Result<Record> {
    let operation = match record.operation {
        0 => Operation::Bind,
        1 => Operation::ReadTimeout,
        2 => Operation::WriteTimeout,
        3 => Operation::Connect,
        4 => Operation::LocalAddress,
        5 => Operation::Send,
        6 => Operation::Receive,
        7 => Operation::Reply,
        8 => Operation::Complete,
        _ => return Err("network_udp_operation_refused"),
    };
    Ok(Record {
        family: record.family,
        operation,
        source_port: record.source_port,
        destination_port: record.destination_port,
        sent: record.sent,
    })
}
fn positives(worker_pin: [u8; 32], protected: Option<Instant>) -> Result<()> {
    let started = Instant::now();
    for p in network_probe::PROBES {
        if let Some(protected) = protected {
            budget(protected, false)?;
        }
        if started.elapsed() >= Duration::from_secs(30) {
            return Err("network_positive_phase_deadline");
        }
        let listener = Listener::new(p)?;
        let mut child = Prepared::new(worker_pin, Scenario::Network(p, listener.port))?;
        child.resume()?;
        let receipt = child.finish(Duration::from_secs(3))?;
        if naturally_finished(&receipt)? != 0 {
            return Err("network_positive_exit_refused");
        }
        if let Some(record) = metadata(&receipt, p, listener.port)? {
            if record.operation != Operation::Complete || record.sent != 4 {
                return Err("network_positive_exchange_unverified");
            }
        }
        listener.finish()?;
        if let Some(protected) = protected {
            budget(protected, false)?;
        }
    }
    Ok(())
}
/// Non-admin authored-worker check only. It cannot create an elevated helper
/// or open WFP; no active guarded-network role is exported here.
pub fn check_network_workers(worker_pin: [u8; 32]) -> Result<()> {
    process::non_admin()?;
    positives(worker_pin, None)?;
    println!("{{\"ok\":true,\"scope\":\"authored_medium_positive_probes_only\",\"checks\":4,\"originalClientTested\":false,\"executionReady\":false}}");
    Ok(())
}

/// Explicit one-shot role only. Errors return to a process that exits; they
/// never wait for the helper's launcher-exit STOPWAIT.
pub(crate) fn launcher(
    launcher_pin: [u8; 32],
    worker_pin: [u8; 32],
    guard_pin: [u8; 32],
) -> Result<()> {
    common::require_supported_os()?;
    process::non_admin()?;
    let owner = process::token_facts(unsafe { GetCurrentProcess() })?;
    if owner.integrity != 8192 {
        return Err("medium_integrity_required");
    }
    let (_, launcher_image, _launcher_locks) = process::fixed_image(Image::Launcher, launcher_pin)?;
    process::image_matches(unsafe { GetCurrentProcess() }, &launcher_image)?;
    let (_, _worker_image, _worker_locks) = process::fixed_image(Image::Worker, worker_pin)?;
    let (guard_path, guard_image, _guard_locks) = process::fixed_image(Image::Guard, guard_pin)?;
    process::verify_guard_load_flags(&guard_image)?;
    // No consent request or high helper exists unless all four before controls pass.
    positives(worker_pin, None)?;
    let nonce = common::nonce()?;
    let pipe = Pipe::server(nonce)?;
    let parent = process::process_stamp(unsafe { GetCurrentProcess() })?;
    let (mut file, _output_locks) = output()?;
    let mut released = false;
    let result = (|| {
        let parameters = process::wide(format!(
            "--network-one-shot {} {} {} {} {} {}",
            common::nonce_text(nonce),
            parent.pid,
            parent.created,
            common::pin_text(launcher_pin),
            common::pin_text(worker_pin),
            common::pin_text(guard_pin)
        ));
        let verb = process::wide("runas");
        let path = process::wide(&guard_path);
        let directory = process::wide(guard_path.parent().ok_or("guard_parent_required")?);
        let mut info = SHELLEXECUTEINFOW::default();
        info.cbSize = std::mem::size_of_val(&info) as u32;
        info.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC;
        info.lpVerb = verb.as_ptr();
        info.lpFile = path.as_ptr();
        info.lpParameters = parameters.as_ptr();
        info.lpDirectory = directory.as_ptr();
        info.nShow = SW_HIDE;
        process::yes(unsafe { ShellExecuteExW(&mut info) })?;
        let retained = Handle::new(info.hProcess)?;
        let started = Instant::now();
        common::hard_deadline()?;
        let identity = process::process_stamp(retained.0)?;
        let helper = process::verify_peer(retained, identity, &guard_image, &owner, true)?;
        pipe.accept()?;
        common::peer_on_pipe(&pipe, &helper)?;
        pipe.receive()?
            .expect(nonce, crate::protocol::Tag::Hello, 0, helper.stamp)?;
        expect(receive(&pipe)?, control(nonce, net::Kind::Ready, 4))?;
        budget(started, true)?;
        for (index, p) in network_probe::PROBES.into_iter().enumerate() {
            budget(started, index == 0)?;
            common::peer_on_pipe(&pipe, &helper)?;
            // The two-second listener is created after UAC and immediately
            // before this probe, never as an array before the consent dialog.
            let listener = Listener::new(p)?;
            let mut child = Prepared::new(worker_pin, Scenario::Network(p, listener.port))?;
            let root = net::Root {
                stamp: stamp(child.stamp()?),
                port: listener.port,
            };
            let seq = 5 + 5 * index as u32;
            send(&pipe, step(nonce, net::Kind::Register, seq, p, root))?;
            expect(
                receive(&pipe)?,
                step(nonce, net::Kind::Ack, seq + 1, p, root),
            )?;
            budget(started, index == 0)?;
            common::peer_on_pipe(&pipe, &helper)?;
            child.resume()?;
            let receipt = child.finish(Duration::from_secs(3))?;
            let code = naturally_finished(&receipt)?;
            let record = metadata(&receipt, p, listener.port)?;
            listener.finish()?;
            budget(started, false)?;
            let mut result = step(nonce, net::Kind::Result, seq + 2, p, root);
            result.raw_exit = code;
            result.udp = record.map(wire_record);
            send(&pipe, result)?;
            expect(
                receive(&pipe)?,
                step(nonce, net::Kind::ProbeDone, seq + 3, p, root),
            )?;
            budget(started, false)?;
            // Authenticated ProbeDone attests the high peer's actual check, not
            // an independent low-side BFE observation. Local cleanup is already verified.
            send(&pipe, step(nonce, net::Kind::Next, seq + 4, p, root))?;
        }
        budget(started, false)?;
        send(&pipe, control(nonce, net::Kind::Release, 25))?;
        expect(receive(&pipe)?, control(nonce, net::Kind::Released, 26))?;
        released = true;
        common::wait_exit(&helper, Instant::now() + Duration::from_secs(3))?;
        let mut code = 0;
        process::yes(unsafe { GetExitCodeProcess(helper.handle.0, &mut code) })?;
        if code != 0 {
            return Err("network_guard_exit_not_success");
        }
        budget(started, false)?;
        positives(worker_pin, Some(started))?;
        budget(started, false)?;
        Ok(())
    })();
    report(&mut file, &result, false, released, released)?;
    result
}

fn actual_exit(peer: &Peer) -> Result<i32> {
    if !process::exited(peer)? {
        return Err("network_target_exit_missing");
    }
    let mut code = 0;
    process::yes(unsafe { GetExitCodeProcess(peer.handle.0, &mut code) })?;
    Ok(code as i32)
}
fn drop_event(observed: kernel::OutboundObservation) -> Result<net::DropEvent> {
    if observed.stats.status != Status::Matched
        || observed.stats.outbound == 0
        || observed.stats.inbound != 0
        || observed.stats.api.is_some()
    {
        return Err("network_outbound_evidence_incomplete");
    }
    let event = observed.event.ok_or("network_actual_event_missing")?;
    Ok(net::DropEvent {
        classify_drop: event.drop,
        presence: event.presence,
        app_matches: event.app_equal,
        family: event.family,
        protocol: event.protocol,
        local_loopback: event.local_loopback,
        remote_loopback: event.remote_loopback,
        local_port: event.local_port,
        remote_port: event.remote_port,
        timestamp: event.timestamp,
        filter_id: event.filter_id,
        layer_id: event.layer_id,
        direction: event.direction,
        is_loopback: event.is_loopback,
        complete: true,
        api_error: None,
    })
}

/// Before release, every error waits for BOTH the
/// launcher and all registered roots: an unregistered suspended next root may
/// exist. The helper never retains a job handle or launches another process.
pub(crate) fn guard(
    nonce: [u8; 16],
    parent: crate::protocol::Stamp,
    launcher_pin: [u8; 32],
    worker_pin: [u8; 32],
    guard_pin: [u8; 32],
) -> Result<()> {
    let started = Instant::now();
    common::require_supported_os()?;
    process::yes(unsafe { SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_SYSTEM32) })?;
    common::hard_deadline()?;
    let owner = process::token_facts(unsafe { GetCurrentProcess() })?;
    if !owner.elevated || owner.integrity != 12288 {
        return Err("high_integrity_required");
    }
    let (_, guard_image, _guard_locks) = process::fixed_image(Image::Guard, guard_pin)?;
    process::verify_guard_load_flags(&guard_image)?;
    process::image_matches(unsafe { GetCurrentProcess() }, &guard_image)?;
    let (_, launcher_image, _launcher_locks) = process::fixed_image(Image::Launcher, launcher_pin)?;
    let launcher = process::open_peer(parent, &launcher_image, &owner, false)?;
    let pipe = Pipe::client(nonce)?;
    common::peer_on_pipe(&pipe, &launcher)?;
    // No privileged output is created until the actual caller and server PID
    // have passed owner/session/token/image admission.
    let (mut file, _output_locks) = output()?;
    let mut release_attempted = false;
    let mut released = false;
    let result = (|| {
        let (worker_path, worker_image, _worker_locks) =
            process::fixed_image(Image::Worker, worker_pin)?;
        pipe.send(crate::protocol::Frame::new(
            crate::protocol::Tag::Hello,
            nonce,
            0,
            process::process_stamp(unsafe { GetCurrentProcess() })?,
            0,
        ))?;
        let mut native = crate::wfp::Native::new(
            metallic_external_guard_fixture::Budget::default(),
            worker_path,
        )?;
        let rules = metallic_external_guard_fixture::install(&mut native)?;
        let mut roots: Vec<Peer> = Vec::new();
        let mut accepted = 0usize;
        let batch = (|| {
            let (filter_ids, layer_ids) = native.scope(&rules)?;
            let scope = net::GuardScope {
                filter_ids,
                layer_ids,
                exact_readback: true,
            };
            budget(started, true)?;
            common::peer_on_pipe(&pipe, &launcher)?;
            send(&pipe, control(nonce, net::Kind::Ready, 4))?;
            for (index, p) in network_probe::PROBES.into_iter().enumerate() {
                budget(started, index == 0)?;
                let registration = receive(&pipe)?;
                common::peer_on_pipe(&pipe, &launcher)?;
                let root = registration
                    .root
                    .ok_or("network_registration_root_missing")?;
                let seq = 5 + 5 * index as u32;
                expect(registration, step(nonce, net::Kind::Register, seq, p, root))?;
                if root.stamp.created < parent.created
                    || roots.iter().any(|peer| peer.stamp.pid == root.stamp.pid)
                {
                    return Err("network_target_creation_or_replay_refused");
                }
                let peer = process::open_peer(legacy(root.stamp), &worker_image, &owner, false)?;
                if process::exited(&peer)? {
                    return Err("network_target_exited_before_ack");
                }
                roots.push(peer);
                let ack_utc = utc()?;
                if ack_utc < root.stamp.created {
                    return Err("network_ack_clock_refused");
                }
                budget(started, index == 0)?;
                send(&pipe, step(nonce, net::Kind::Ack, seq + 1, p, root))?;
                let result = receive(&pipe)?;
                common::peer_on_pipe(&pipe, &launcher)?;
                budget(started, false)?;
                let mut expected = step(nonce, net::Kind::Result, seq + 2, p, root);
                expected.raw_exit = result.raw_exit;
                expected.udp = result.udp;
                expect(result, expected)?;
                if !p.tcp() && result.udp.is_none() {
                    return Err("network_udp_metadata_missing");
                }
                let peer = roots.last().unwrap();
                let code = actual_exit(peer)?;
                let end = utc()?;
                let event = if code == (0x10000 | 10060) {
                    let record = kernel_record(result.udp.ok_or("network_udp_metadata_missing")?)?;
                    Some(drop_event(native.outbound(&rules, record, ack_utc, end))?)
                } else {
                    None
                };
                net::blocked_outcome(
                    probe(p),
                    root,
                    ack_utc,
                    scope,
                    result.raw_exit,
                    result.udp,
                    net::ExitFacts {
                        stamp: stamp(peer.stamp),
                        actual_exit: Some(code),
                        observed_utc: end,
                        drop: event,
                    },
                )?;
                accepted += 1;
                send(&pipe, step(nonce, net::Kind::ProbeDone, seq + 3, p, root))?;
                expect(
                    receive(&pipe)?,
                    step(nonce, net::Kind::Next, seq + 4, p, root),
                )?;
                common::peer_on_pipe(&pipe, &launcher)?;
                budget(started, false)?;
            }
            expect(receive(&pipe)?, control(nonce, net::Kind::Release, 25))?;
            common::peer_on_pipe(&pipe, &launcher)?;
            budget(started, false)?;
            if accepted != 4 || roots.len() != 4 {
                return Err("network_batch_incomplete");
            }
            for peer in &roots {
                if !process::exited(peer)? {
                    return Err("network_release_live_target");
                }
            }
            // Cleanup may partially remove filters before failing. From this
            // point, report a release attempt truthfully, never promise that a
            // later transport failure kept all four guards installed.
            release_attempted = true;
            metallic_external_guard_fixture::cleanup(&mut native, &rules, true)?;
            released = true;
            send(&pipe, control(nonce, net::Kind::Released, 26))?;
            Ok(())
        })();
        if batch.is_err() && !release_attempted {
            loop {
                let parent_exited = process::exited(&launcher) == Ok(true);
                let all_roots_exited = roots.iter().all(|peer| process::exited(peer) == Ok(true));
                if parent_exited && all_roots_exited {
                    break;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            // A latched soft budget can make cleanup fail; dynamic-session
            // Drop remains best effort. This path can never report success.
            let _ = metallic_external_guard_fixture::cleanup(&mut native, &rules, true);
        }
        batch
    })();
    report(&mut file, &result, true, release_attempted, released)?;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn binding_and_time_guards_are_closed() {
        let n = [7; 16];
        let f = control(n, net::Kind::Ready, 4);
        assert_eq!(expect(f, f), Ok(()));
        let mut wrong = f;
        wrong.seq = 5;
        assert!(expect(wrong, f).is_err());
        wrong = f;
        wrong.nonce[0] ^= 1;
        assert!(expect(wrong, f).is_err());
        assert_eq!(budget(Instant::now(), true), Ok(()));
        assert!(budget(Instant::now() - Duration::from_secs(30), true).is_err());
        assert!(budget(Instant::now() - Duration::from_secs(90), false).is_err());
    }
    #[test]
    fn event_adapter_never_invents_missing_or_incomplete_native_fields() {
        assert!(drop_event(kernel::OutboundObservation::unavailable(
            Status::Inconclusive
        ))
        .is_err());
        let matched = kernel::Stats {
            status: Status::Matched,
            outbound: 1,
            inbound: 0,
            api: None,
        };
        assert!(drop_event(kernel::OutboundObservation {
            stats: matched,
            event: None
        })
        .is_err());
        let event = kernel::Event {
            drop: true,
            presence: 0x13f,
            family: 4,
            protocol: 17,
            local_loopback: true,
            remote_loopback: true,
            local_port: 123,
            remote_port: 456,
            app_equal: true,
            timestamp: 101,
            filter_id: 77,
            layer_id: 48,
            direction: 0x3901,
            is_loopback: true,
        };
        let converted = drop_event(kernel::OutboundObservation {
            stats: matched,
            event: Some(event),
        })
        .unwrap();
        assert_eq!(converted.timestamp, event.timestamp);
        assert_eq!(converted.filter_id, event.filter_id);
        assert_eq!(converted.local_port, event.local_port);
        for status in [Status::Capped, Status::Deadline, Status::Unavailable] {
            assert!(drop_event(kernel::OutboundObservation {
                stats: kernel::Stats::empty(status),
                event: Some(event)
            })
            .is_err());
        }
    }
}
