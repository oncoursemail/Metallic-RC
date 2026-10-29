#[cfg(windows)]fn checks(pin:[u8;32])->metallic_owned_process_adapter::Result<()>{
    use metallic_owned_process_adapter::{Prepared,Scenario};use std::time::{Duration,Instant};
    let mut wrong=pin;wrong[0]^=1;if !matches!(Prepared::new(wrong,Scenario::Marker),Err("worker_pin_mismatch")){return Err("wrong_pin_refusal_not_verified")}
    let mut p=Prepared::new(pin,Scenario::Marker)?;let image=std::env::current_exe().map_err(|_|"checker_image_unavailable")?.parent().ok_or("checker_parent_unavailable")?.join(metallic_owned_process_adapter::WORKER);if std::fs::OpenOptions::new().write(true).open(image).is_ok(){return Err("worker_write_lock_not_held")}std::thread::sleep(Duration::from_millis(40));if p.output_counts()!=(0,0){return Err("suspended_worker_output")}
    p.resume()?;if p.resume().is_ok(){return Err("double_resume_accepted")}let r=p.finish(Duration::from_secs(3))?;if r.exit_code!=Some(0)||!r.stdout.expected||!r.stdout.eof||!r.stderr.eof||!r.job_empty{return Err("marker_check_failed")}
    let previous=std::env::var_os("ADAPTER_SYNTHETIC_FORBIDDEN");std::env::set_var("ADAPTER_SYNTHETIC_FORBIDDEN","synthetic-omit");let prepared=Prepared::new(pin,Scenario::Environment);if let Some(value)=previous{std::env::set_var("ADAPTER_SYNTHETIC_FORBIDDEN",value)}else{std::env::remove_var("ADAPTER_SYNTHETIC_FORBIDDEN")};let mut p=prepared?;p.resume()?;let r=p.finish(Duration::from_secs(3))?;if r.exit_code!=Some(0)||!r.stdout.expected{return Err("environment_scrub_failed")}
    let mut p=Prepared::new(pin,Scenario::Flood)?;p.resume()?;let r=p.finish(Duration::from_secs(5))?;if r.exit_code!=Some(0)||r.stdout.bytes!=2097152||r.stderr.bytes!=2097152||r.stdout.prefix_bytes!=65536||r.stderr.prefix_bytes!=65536||!r.stdout.eof||!r.stderr.eof{return Err("concurrent_output_bound_failed")}if r.stdout.sha256!=metallic_owned_process_adapter::digest("156c38442089c1323d3e3ba549a6ac24341c47e8b6367bec4740c9b8c865826e")?||r.stderr.sha256!=metallic_owned_process_adapter::digest("fee47b1f0d7685a226fd5f2b9dd8f525038bbb05fe9d89a5d75c249edac868e3")?{return Err("output_prefix_hash_mismatch")}
    let p=Prepared::new(pin,Scenario::Marker)?;let r=p.finish(Duration::ZERO)?;if r.resumed||r.stdout.bytes!=0||!r.job_empty{return Err("unresumed_cleanup_failed")}
    let mut p=Prepared::new(pin,Scenario::Grandchild)?;p.resume()?;let end=Instant::now()+Duration::from_secs(2);while (p.process_count()?<2||p.output_counts().0<14)&&Instant::now()<end{std::thread::sleep(Duration::from_millis(5));}if p.process_count()?<2||p.output_counts().0!=14{return Err("grandchild_not_observed")}let r=p.finish(Duration::from_millis(100))?;if !r.timed_out||r.before_cleanup.is_some()||r.exit_code!=Some(124)||!r.job_empty||!r.stdout.eof||!r.stderr.eof||!r.stdout.expected{return Err("timeout_tree_cleanup_failed")}
    parent_death(pin)?;
    metallic_owned_process_adapter::check_drain_failure(pin,false)?;metallic_owned_process_adapter::check_drain_failure(pin,true)?;
    println!("{{\"ok\":true,\"scope\":\"authored_sibling_worker_only\",\"checks\":9,\"originalClientTested\":false,\"executionReady\":false}}");Ok(())
}
#[cfg(windows)]fn death_owner(pin:[u8;32])->metallic_owned_process_adapter::Result<()>{
    use metallic_owned_process_adapter::{Prepared,Scenario};use std::{io::{Read,Write},time::Duration};
    let mut p=Prepared::new(pin,Scenario::Wait)?;p.resume()?;let pids=p.owned_pids()?;if pids.len()!=1{return Err("death_owner_tree_not_observed")}
    println!("{}",pids[0]);std::io::stdout().flush().map_err(|_|"death_owner_metadata_failed")?;
    let(tx,rx)=std::sync::mpsc::channel();std::thread::spawn(move||{let mut b=[0];let _=tx.send(std::io::stdin().read_exact(&mut b).is_ok()&&b==[b'D']);});
    if rx.recv_timeout(Duration::from_secs(2))!=Ok(true){return Err("death_owner_handshake_failed")}
    // Intentionally bypass Rust Drop to test the OS last-handle job teardown.
    std::process::exit(0)
}
#[cfg(windows)]fn parent_death(pin:[u8;32])->metallic_owned_process_adapter::Result<()>{
    use std::{io::{BufRead,Read,Write},os::windows::process::CommandExt,process::{Command,Stdio},time::{Duration,Instant}};
    let text=pin.iter().map(|b|format!("{b:02x}")).collect::<String>();let mut owner=Command::new(std::env::current_exe().map_err(|_|"checker_image_unavailable")?).args(["--death-owner",&text]).creation_flags(0x08000000).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().map_err(|_|"death_owner_spawn_failed")?;
    let result=(||{let output=owner.stdout.take().ok_or("death_owner_stdout_missing")?;let(tx,rx)=std::sync::mpsc::channel();let reader=std::thread::spawn(move||{let mut line=String::new();let _=tx.send(std::io::BufReader::new(output).take(128).read_line(&mut line).map(|_|line));});
        let line=rx.recv_timeout(Duration::from_secs(2)).map_err(|_|"death_owner_metadata_deadline")?.map_err(|_|"death_owner_metadata_failed")?;reader.join().map_err(|_|"death_owner_reader_failed")?;
        let pids=line.split_whitespace().map(|s|s.parse::<u32>().map_err(|_|"death_owner_pid_invalid")).collect::<metallic_owned_process_adapter::Result<Vec<_>>>()?;if pids.len()!=1{return Err("death_owner_pid_count")}
        let retained=pids.into_iter().map(|pid|metallic_owned_process_adapter::retained_worker(pid,pin)).collect::<metallic_owned_process_adapter::Result<Vec<_>>>()?;
        owner.stdin.take().ok_or("death_owner_stdin_missing")?.write_all(b"D").map_err(|_|"death_owner_ack_failed")?;
        let end=Instant::now()+Duration::from_secs(2);while owner.try_wait().map_err(|_|"death_owner_wait_failed")?.is_none()&&Instant::now()<end{std::thread::sleep(Duration::from_millis(5));}
        if owner.try_wait().map_err(|_|"death_owner_wait_failed")?.and_then(|s|s.code())!=Some(0){return Err("death_owner_exit_failed")}
        if retained.iter().any(|h|!metallic_owned_process_adapter::wait_retained(h)){return Err("job_close_did_not_reap_tree")}Ok(())
    })();if !matches!(owner.try_wait(),Ok(Some(_))){let _=owner.kill();}let end=Instant::now()+Duration::from_secs(2);while !matches!(owner.try_wait(),Ok(Some(_)))&&Instant::now()<end{std::thread::sleep(Duration::from_millis(5));}if !matches!(owner.try_wait(),Ok(Some(_))){return Err("death_owner_cleanup_deadline")}result
}
fn main(){
    let args:Vec<String>=std::env::args().skip(1).collect();
    if args==["--plan"]{println!("{{\"gameWillNotRun\":true,\"executionReady\":false,\"target\":\"fixed_authored_sibling_worker_only\"}}");return}
    #[cfg(windows)]let result=match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice(){["--self-test",pin]=>metallic_owned_process_adapter::digest(pin).and_then(checks),["--death-owner",pin]=>metallic_owned_process_adapter::digest(pin).and_then(death_owner),_=>Err("closed_self_test_arguments_required")};
    #[cfg(not(windows))]let result:metallic_owned_process_adapter::Result<()>=Err("windows_only");
    if let Err(reason)=result{eprintln!("{{\"ok\":false,\"error\":\"{reason}\",\"originalClientTested\":false,\"executionReady\":false}}");std::process::exit(1)}
}
