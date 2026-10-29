//! Native fixture orchestration; never called by plan or unit-test roles.
use metallic_external_guard_fixture::{classify_probe, cleanup, finish_probe, install, resolve_probe, run_probe_phase, timeouts_ready, Budget, Probe, ProbeFailure, ProbePhase, Result};
use std::{io::{Read, Write}, net::{SocketAddr, TcpListener, TcpStream, UdpSocket}, os::windows::{io::AsRawHandle, process::CommandExt}, process::{Child, Command, Stdio}, ptr, sync::mpsc, thread, time::{Duration, Instant}};
use windows_sys::Win32::{Foundation::{CloseHandle, GetLastError, HANDLE}, System::{JobObjects::*, Threading::CreateMutexW}};
use crate::wfp::Native;
use metallic_external_guard_fixture::kernel::{Observation,Operation,Record,FRAME_BYTES};
use windows_sys::Win32::System::Pipes::PeekNamedPipe;

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
    let mut metadata=Record::new(probe,port);
    let exchanged=(||->std::io::Result<bool>{
        if probe.tcp(){
            let mut s=TcpStream::connect_timeout(&address,Duration::from_millis(200))?;
            s.set_read_timeout(Some(Duration::from_millis(200)))?;s.set_write_timeout(Some(Duration::from_millis(200)))?;
            s.write_all(b"MRC1")?;let mut b=[0;4];s.read_exact(&mut b)?;Ok(b==*b"MRC1")
        }else{
            let record=metadata.as_mut().unwrap();
            let bind=SocketAddr::new(probe.host().parse().unwrap(),0);let s=UdpSocket::bind(bind)?;
            record.source_port=s.local_addr()?.port();
            record.operation=Operation::ReadTimeout;s.set_read_timeout(Some(Duration::from_millis(200)))?;
            record.operation=Operation::WriteTimeout;s.set_write_timeout(Some(Duration::from_millis(200)))?;
            record.operation=Operation::Connect;s.connect(address)?;
            record.operation=Operation::LocalAddress;let local=s.local_addr()?;
            if local.ip()!=bind.ip()||local.port()!=record.source_port{return Ok(false)}
            record.operation=Operation::Send;record.sent=s.send(b"MRC1")?.try_into().unwrap_or(u8::MAX);
            if record.sent!=4{return Ok(false)}
            record.operation=Operation::Receive;let mut b=[0;4];let n=s.recv(&mut b)?;
            record.operation=Operation::Reply;let ok=n==4&&b==*b"MRC1";
            if ok{record.operation=Operation::Complete;}Ok(ok)
        }
    })();
    let code=classify_probe(exchanged.map_err(|error|error.raw_os_error()));
    // One tiny frame, explicitly flushed before process::exit; no new success
    // path. A failed write/flush is an explicit generic failure.
    if let Some(record)=metadata{if std::io::stdout().write_all(&record.encode()).and_then(|_|std::io::stdout().flush()).is_err(){return 21}}
    code
}

fn utc_ticks()->Option<u64>{
    let since=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok()?;
    since.as_secs().checked_add(11644473600)?.checked_mul(10_000_000)?.checked_add(since.subsec_nanos()as u64/100)
}
fn read_metadata<R:Read+AsRawHandle>(pipe:&mut R,probe:Probe,port:u16)->Option<Record>{
    let mut available=0;
    if unsafe{PeekNamedPipe(pipe.as_raw_handle() as HANDLE,ptr::null_mut(),0,ptr::null_mut(),&mut available,ptr::null_mut())}==0||available!=FRAME_BYTES as u32{return None}
    let mut frame=[0;FRAME_BYTES];pipe.read_exact(&mut frame).ok()?;
    let mut trailing=0;
    if unsafe{PeekNamedPipe(pipe.as_raw_handle() as HANDLE,ptr::null_mut(),0,ptr::null_mut(),&mut trailing,ptr::null_mut())}!=0{
        if trailing!=0{return None}
    }else if unsafe{GetLastError()}!=109{return None}
    Record::decode(&frame,probe,port)
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
    fn owned_pipe(bytes:&[u8])->std::fs::File{
        use std::os::windows::io::FromRawHandle;
        let(mut read,mut write)=(ptr::null_mut(),ptr::null_mut());
        assert_ne!(unsafe{windows_sys::Win32::System::Pipes::CreatePipe(&mut read,&mut write,ptr::null(),0)},0);
        let mut writer=unsafe{std::fs::File::from_raw_handle(write as _)};writer.write_all(bytes).unwrap();drop(writer);
        unsafe{std::fs::File::from_raw_handle(read as _)}
    }
    #[test]fn udp_metadata_pipe_exact_closed_writer(){
        let record=Record{family:4,operation:Operation::Receive,source_port:42123,destination_port:52123,sent:4};
        let mut pipe=owned_pipe(&record.encode());assert_eq!(read_metadata(&mut pipe,Probe::Udp4,52123),Some(record));
    }
    #[test]fn udp_metadata_pipe_short_or_extra_is_unavailable(){
        let frame=Record{family:4,operation:Operation::Receive,source_port:42123,destination_port:52123,sent:4}.encode();
        for bytes in [vec![],vec![0],frame[..31].to_vec(),[frame.as_slice(),&[0]].concat(),[frame,frame].concat()]{let mut pipe=owned_pipe(&bytes);assert_eq!(read_metadata(&mut pipe,Probe::Udp4,52123),None);}
    }
    #[test]fn udp_metadata_pipe_read_failure_and_wrong_destination_are_unavailable(){
        struct FailedRead(std::fs::File);
        impl Read for FailedRead{fn read(&mut self,_:&mut[u8])->std::io::Result<usize>{Err(std::io::Error::from(std::io::ErrorKind::UnexpectedEof))}}
        impl AsRawHandle for FailedRead{fn as_raw_handle(&self)->std::os::windows::io::RawHandle{self.0.as_raw_handle()}}
        let frame=Record{family:4,operation:Operation::Receive,source_port:42123,destination_port:52123,sent:4}.encode();
        let mut failed=FailedRead(owned_pipe(&frame));assert_eq!(read_metadata(&mut failed,Probe::Udp4,52123),None);
        let mut pipe=owned_pipe(&frame);assert_eq!(read_metadata(&mut pipe,Probe::Udp4,52124),None);
    }
}

fn probe(job:&Job,children:&mut Children,p:Probe,phase:ProbePhase,budget:&Budget,failure:&mut Option<ProbeFailure>,guard:Option<(&Native,&[metallic_external_guard_fixture::Rule])>)->Result<()>{
    *failure=Some(ProbeFailure{phase,protocol:p,actual:None,udp:None});
    budget.check()?;
    let(port,stop,server)=listener(p)?;
    let outcome=(||{
        // This is the only spawn target. The child waits for G until assigned.
        let exe=std::env::current_exe().map_err(|_|"current_exe_failed")?;
        budget.check()?;
        children.0.push(Command::new(exe).args(["--probe",p.name(),&port.to_string()]).creation_flags(0x08000000).stdin(Stdio::piped()).stdout(if p.tcp(){Stdio::null()}else{Stdio::piped()}).stderr(Stdio::null()).spawn().map_err(|_|"probe_spawn_failed")?);
        let child=children.0.last_mut().unwrap();job.assign(child)?;
        budget.check()?;
        let start=utc_ticks().unwrap_or(0);
        child.stdin.take().ok_or("probe_input_missing")?.write_all(b"G").map_err(|_|"probe_handshake_failed")?;
        let end=Instant::now()+Duration::from_secs(2);
        while Instant::now()<end{
            if let Some(status)=child.try_wait().map_err(|_|"probe_wait_failed")?{
                let udp=if p.tcp(){None}else{Some(Observation{record:if status.code().is_some(){child.stdout.take().and_then(|mut pipe|read_metadata(&mut pipe,p,port))}else{None},start,end:utc_ticks().unwrap_or(0),..Default::default()})};
                *failure=Some(ProbeFailure{phase,protocol:p,actual:status.code(),udp});
                budget.check()?;
                let decision=resolve_probe(failure.as_mut().ok_or("probe_context_missing")?,|observation|match guard{
                    Some((api,rules))=>api.diagnose_udp(rules,observation),
                    None=>metallic_external_guard_fixture::kernel::Stats::empty(metallic_external_guard_fixture::kernel::Status::Unavailable),
                });
                budget.check()?;return decision
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
        run_probe_phase(ProbePhase::Before,|p,phase|probe(&job,&mut children,p,phase,&budget,probe_failure,None))?;
        if !children.empty()||!job.empty(){return Err("baseline_children_pending")}
        rules=Some(install(&mut api)?);
        budget.check()?;let verified=rules.as_deref().ok_or("verified_rules_missing")?;
        run_probe_phase(ProbePhase::Blocked,|p,phase|probe(&job,&mut children,p,phase,&budget,probe_failure,Some((&api,verified))))?;
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
    let after=(||{outcome?;removed?;budget.check()?;run_probe_phase(ProbePhase::After,|p,phase|probe(&job,&mut children,p,phase,&budget,probe_failure,None))?;budget.check()})();
    children.stop();let _=job.stop();let end=Instant::now()+Duration::from_secs(3);
    while Instant::now()<end&&(!children.empty()||!job.empty()){pause();}
    if !children.empty()||!job.empty(){loop{thread::sleep(Duration::from_millis(100));}}
    *observed_flags=api.last_observed_flags();
    watchdog.finish()?;budget.check()?;after
}
