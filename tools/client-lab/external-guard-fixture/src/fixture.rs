//! Native fixture orchestration; never called by plan or unit-test roles.
use metallic_external_guard_fixture::{accepted_probe_exit, classify_probe, cleanup, finish_probe, install, timeouts_ready, Budget, Probe, ProbeFailure, ProbePhase, Result, PROBES};
use std::{io::{Read, Write}, net::{SocketAddr, TcpListener, TcpStream, UdpSocket}, os::windows::{io::AsRawHandle, process::CommandExt}, process::{Child, Command, Stdio}, ptr, sync::mpsc, thread, time::{Duration, Instant}};
use windows_sys::Win32::{Foundation::{CloseHandle, GetLastError, HANDLE}, System::{JobObjects::*, Threading::CreateMutexW}};
use crate::wfp::Native;

fn pause(){thread::sleep(Duration::from_millis(10));}
fn os_ok(ok:i32)->Result<()>{if ok!=0{Ok(())}else{Err("owned_windows_operation_failed")}}
struct Lock(HANDLE);
impl Lock {
    fn take()->Result<Self>{
        let name:Vec<u16>="Global\\Metallic-Owned-ExternalGuard-Fixture-v1".encode_utf16().chain(Some(0)).collect();
        let h=unsafe{CreateMutexW(ptr::null(),0,name.as_ptr())};
        if h.is_null(){return Err("fixture_mutex_refused")}
        if unsafe{GetLastError()}==183{unsafe{CloseHandle(h)};return Err("fixture_already_running")}
        Ok(Self(h))
    }
}
impl Drop for Lock{fn drop(&mut self){unsafe{CloseHandle(self.0)};}}
struct Job(HANDLE);
impl Job {
    fn new()->Result<Self>{
        let h=unsafe{CreateJobObjectW(ptr::null(),ptr::null())};
        if h.is_null(){return Err("job_create_failed")}
        let job=Self(h);
        let mut info:JOBOBJECT_EXTENDED_LIMIT_INFORMATION=unsafe{std::mem::zeroed()};
        info.BasicLimitInformation.LimitFlags=JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        os_ok(unsafe{SetInformationJobObject(h,JobObjectExtendedLimitInformation,&info as *const _ as *const _,std::mem::size_of_val(&info) as u32)})?;
        Ok(job)
    }
    fn assign(&self,child:&Child)->Result<()>{os_ok(unsafe{AssignProcessToJobObject(self.0,child.as_raw_handle() as HANDLE)})}
    fn stop(&self)->Result<()>{os_ok(unsafe{TerminateJobObject(self.0,124)})}
    fn empty(&self)->bool{
        let mut info:JOBOBJECT_BASIC_ACCOUNTING_INFORMATION=unsafe{std::mem::zeroed()};
        unsafe{QueryInformationJobObject(self.0,JobObjectBasicAccountingInformation,&mut info as *mut _ as *mut _,std::mem::size_of_val(&info) as u32,ptr::null_mut())!=0 && info.ActiveProcesses==0}
    }
}
impl Drop for Job{fn drop(&mut self){let _=self.stop();unsafe{CloseHandle(self.0)};}}

struct Watchdog { stop:Option<mpsc::Sender<()>>, thread:Option<thread::JoinHandle<()>> }
impl Watchdog {
    fn start(job:&Job,budget:Budget)->Self{
        let (tx,rx)=mpsc::channel();let h=job.0 as usize;
        let thread=thread::spawn(move||{
            match rx.recv_timeout(Duration::from_secs(90)){Err(mpsc::RecvTimeoutError::Timeout)=>{},_=>return}
            budget.expire();
            unsafe{TerminateJobObject(h as HANDLE,124)};
            if matches!(rx.recv_timeout(Duration::from_secs(40)),Err(mpsc::RecvTimeoutError::Timeout)){
                std::process::exit(124);
            }
        });Self{stop:Some(tx),thread:Some(thread)}
    }
    fn finish(&mut self)->Result<()>{if let Some(tx)=self.stop.take(){let _=tx.send(());}if let Some(t)=self.thread.take(){t.join().map_err(|_|"watchdog_failed")?;}Ok(())}
}
impl Drop for Watchdog{fn drop(&mut self){let _=self.finish();}}

#[derive(Default)]
struct Children(Vec<Child>);
impl Children {
    fn empty(&mut self)->bool{self.0.iter_mut().all(|p|matches!(p.try_wait(),Ok(Some(_))))}
    fn stop(&mut self){for p in &mut self.0{if !matches!(p.try_wait(),Ok(Some(_))){let _=p.kill();}}}
}
impl Drop for Children{fn drop(&mut self){self.stop();}}

/// A manually invoked internal probe is still restricted to a fixed four-byte
/// synthetic exchange on loopback. It has no WFP or arbitrary command API.
pub fn child(probe:Probe,port:u16)->i32{
    let(tx,rx)=mpsc::channel();
    thread::spawn(move||{let mut b=[0];let ok=std::io::stdin().read_exact(&mut b).is_ok()&&b==[b'G'];let _=tx.send(ok);});
    if rx.recv_timeout(Duration::from_millis(500))!=Ok(true){return 1}
    let address=SocketAddr::new(probe.host().parse().unwrap(),port);
    let exchanged=(||->std::io::Result<bool>{
        if probe.tcp(){
            let mut s=TcpStream::connect_timeout(&address,Duration::from_millis(200))?;
            s.set_read_timeout(Some(Duration::from_millis(200)))?;s.set_write_timeout(Some(Duration::from_millis(200)))?;
            s.write_all(b"MRC1")?;let mut b=[0;4];s.read_exact(&mut b)?;Ok(b==*b"MRC1")
        }else{
            let bind=SocketAddr::new(probe.host().parse().unwrap(),0);let s=UdpSocket::bind(bind)?;
            s.set_read_timeout(Some(Duration::from_millis(200)))?;s.set_write_timeout(Some(Duration::from_millis(200)))?;
            s.connect(address)?;s.send(b"MRC1")?;let mut b=[0;4];Ok(s.recv(&mut b)?==4&&b==*b"MRC1")
        }
    })();
    classify_probe(exchanged.map_err(|error|error.raw_os_error()))
}

fn listener(probe:Probe)->Result<(u16,mpsc::Sender<()>,thread::JoinHandle<Result<()>>)>{
    let address=SocketAddr::new(probe.host().parse().unwrap(),0);
    let(tx,rx)=mpsc::channel();
    if probe.tcp(){
        let s=TcpListener::bind(address).map_err(|_|"tcp_listener_unavailable")?;s.set_nonblocking(true).map_err(|_|"listener_setup_failed")?;
        let port=s.local_addr().map_err(|_|"listener_address_failed")?.port();
        Ok((port,tx,thread::spawn(move||{
            let end=Instant::now()+Duration::from_secs(2);
            while Instant::now()<end&&rx.try_recv().is_err(){
                if let Ok((mut stream,_))=s.accept(){
                    configure_accepted(&stream,Duration::from_millis(100))?;
                    let mut b=[0;4];if stream.read_exact(&mut b).is_ok()&&b==*b"MRC1"{let _=stream.write_all(b"MRC1");}break;
                }pause();
            }Ok(())
        })))
    }else{
        let s=UdpSocket::bind(address).map_err(|_|"udp_listener_unavailable")?;s.set_nonblocking(true).map_err(|_|"listener_setup_failed")?;
        let port=s.local_addr().map_err(|_|"listener_address_failed")?.port();
        Ok((port,tx,thread::spawn(move||{
            let end=Instant::now()+Duration::from_secs(2);let mut b=[0;4];
            while Instant::now()<end&&rx.try_recv().is_err(){if let Ok((4,from))=s.recv_from(&mut b){if b==*b"MRC1"{let _=s.send_to(b"MRC1",from);}break;}pause();}Ok(())
        })))
    }
}

fn configure_accepted(stream:&TcpStream,timeout:Duration)->Result<()>{
    if timeout.is_zero()||timeout>Duration::from_secs(2){return Err("accepted_timeout_bound")}
    stream.set_nonblocking(false).map_err(|_|"accepted_blocking_mode_failed")?;
    timeouts_ready(stream.set_read_timeout(Some(timeout)).is_ok(),stream.set_write_timeout(Some(timeout)).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn owned_pair()->(TcpStream,TcpStream){
        let listener=TcpListener::bind("127.0.0.1:0").unwrap();listener.set_nonblocking(true).unwrap();
        let client=TcpStream::connect_timeout(&listener.local_addr().unwrap(),Duration::from_secs(1)).unwrap();
        client.set_write_timeout(Some(Duration::from_millis(500))).unwrap();
        let end=Instant::now()+Duration::from_secs(1);
        loop{
            match listener.accept(){
                Ok((accepted,_))=>{
                    // Reproduce the documented inherited mode deterministically.
                    accepted.set_nonblocking(true).unwrap();return(client,accepted)
                },
                Err(error) if error.kind()==std::io::ErrorKind::WouldBlock&&Instant::now()<end=>pause(),
                Err(_)=>panic!("owned loopback accept failed"),
            }
        }
    }
    #[test]
    fn accepted_stream_waits_for_delayed_marker(){
        let(mut client,mut accepted)=owned_pair();
        configure_accepted(&accepted,Duration::from_millis(500)).unwrap();
        let(started_tx,started_rx)=mpsc::channel();let(done_tx,done_rx)=mpsc::channel();
        let reader=thread::spawn(move||{
            let _=started_tx.send(());let mut marker=[0;4];
            let result=accepted.read_exact(&mut marker).map(|_|marker).map_err(|error|error.kind());
            let _=done_tx.send(result);
        });
        if started_rx.recv_timeout(Duration::from_secs(1)).is_err(){
            drop(client);reader.join().unwrap();panic!("owned reader did not start")
        }
        // No marker can arrive before this explicit test gate.
        let early=done_rx.recv_timeout(Duration::from_millis(50));
        let waited=matches!(early,Err(mpsc::RecvTimeoutError::Timeout));
        let result=match early{
            Ok(result)=>result,
            Err(mpsc::RecvTimeoutError::Timeout)=>{
                let sent=client.write_all(b"MRC1");drop(client);
                let result=done_rx.recv_timeout(Duration::from_secs(1));
                reader.join().unwrap();assert!(sent.is_ok());
                assert!(waited,"accepted read did not wait for marker");
                assert_eq!(result.unwrap(),Ok(*b"MRC1"));return
            },
            Err(_)=>{reader.join().unwrap();panic!("owned reader disconnected")}
        };
        drop(client);reader.join().unwrap();
        assert!(waited,"accepted read completed before delayed marker: {result:?}");
    }
    #[test]
    fn accepted_stream_timeout_bounds_are_finite(){
        let(_client,accepted)=owned_pair();
        assert_eq!(configure_accepted(&accepted,Duration::ZERO),Err("accepted_timeout_bound"));
        assert_eq!(configure_accepted(&accepted,Duration::from_secs(3)),Err("accepted_timeout_bound"));
        configure_accepted(&accepted,Duration::from_millis(100)).unwrap();
        assert_eq!(accepted.read_timeout().unwrap(),Some(Duration::from_millis(100)));
        assert_eq!(accepted.write_timeout().unwrap(),Some(Duration::from_millis(100)));
    }
}

fn probe(job:&Job,children:&mut Children,p:Probe,phase:ProbePhase,budget:&Budget,failure:&mut Option<ProbeFailure>)->Result<()>{
    *failure=Some(ProbeFailure{phase,protocol:p,actual:None});
    let expected=phase.expected();
    budget.check()?;
    let(port,stop,server)=listener(p)?;
    let outcome=(||{
        // This is the only spawn target. The child waits for G until assigned.
        let exe=std::env::current_exe().map_err(|_|"current_exe_failed")?;
        budget.check()?;
        children.0.push(Command::new(exe).args(["--probe",p.name(),&port.to_string()]).creation_flags(0x08000000).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().map_err(|_|"probe_spawn_failed")?);
        let child=children.0.last_mut().unwrap();job.assign(child)?;
        budget.check()?;
        child.stdin.take().ok_or("probe_input_missing")?.write_all(b"G").map_err(|_|"probe_handshake_failed")?;
        let end=Instant::now()+Duration::from_secs(2);
        while Instant::now()<end{
            if let Some(status)=child.try_wait().map_err(|_|"probe_wait_failed")?{
                *failure=Some(ProbeFailure{phase,protocol:p,actual:status.code()});
                budget.check()?;return if accepted_probe_exit(status.code(),expected){Ok(())}else{Err("probe_outcome_mismatch")}
            }pause();
        }
        Err("probe_deadline")
    })();
    let _=stop.send(());let server_ok=matches!(server.join(),Ok(Ok(())));
    if !server_ok{return Err("listener_thread_failed")}
    budget.check()?;finish_probe(outcome,failure)
}

pub fn run(observed_flags:&mut Option<u32>,probe_failure:&mut Option<ProbeFailure>)->Result<()>{
    *observed_flags=None;
    *probe_failure=None;
    let _single=Lock::take()?;
    let budget=Budget::default();let job=Job::new()?;let mut watchdog=Watchdog::start(&job,budget.clone());
    let mut children=Children::default();let mut api=Native::new(budget.clone())?;let mut rules=None;
    let outcome=(||{
        for p in PROBES{probe(&job,&mut children,p,ProbePhase::Before,&budget,probe_failure)?;}
        if !children.empty()||!job.empty(){return Err("baseline_children_pending")}
        rules=Some(install(&mut api)?);
        budget.check()?;for p in PROBES{probe(&job,&mut children,p,ProbePhase::Blocked,&budget,probe_failure)?;}
        Ok(())
    })();
    *observed_flags=api.last_observed_flags();
    children.stop();let _=job.stop();
    let end=Instant::now()+Duration::from_secs(3);
    while Instant::now()<end&&(!children.empty()||!job.empty()){pause();}
    if !children.empty()||!job.empty(){
        // Keep the dynamic-session owner alive. The 90/130s watchdog handles
        // failure; do not voluntarily drop the guard over pending children.
        loop{thread::sleep(Duration::from_millis(100));}
    }
    let removed=if let Some(rules)=&rules{cleanup(&mut api,rules,true)}else{Ok(())};
    let after=(||{outcome?;removed?;budget.check()?;for p in PROBES{probe(&job,&mut children,p,ProbePhase::After,&budget,probe_failure)?;}budget.check()})();
    children.stop();let _=job.stop();let end=Instant::now()+Duration::from_secs(3);
    while Instant::now()<end&&(!children.empty()||!job.empty()){pause();}
    if !children.empty()||!job.empty(){loop{thread::sleep(Duration::from_millis(100));}}
    *observed_flags=api.last_observed_flags();
    watchdog.finish()?;budget.check()?;after
}
