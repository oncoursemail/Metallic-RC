use crate::{Result,protocol::{Frame,BYTES},process::{Handle,Security,wide,yes}};
use std::{ptr,time::{Duration,Instant}};
use windows_sys::Win32::{Foundation::*,Storage::FileSystem::*,System::{IO::*,Pipes::*,Threading::*}};
pub(crate) struct Pipe{handle:Handle,server:bool}
struct Operation{overlapped:OVERLAPPED,buffer:[u8;256],_event:Handle,pipe:Handle}
#[cfg(test)]static OPERATIONS:std::sync::atomic::AtomicUsize=std::sync::atomic::AtomicUsize::new(0);
impl Drop for Operation{fn drop(&mut self){#[cfg(test)]{OPERATIONS.fetch_sub(1,std::sync::atomic::Ordering::SeqCst);}}}
impl Pipe{
 pub fn name(nonce:[u8;16])->String{format!(r"\\.\pipe\Metallic-Authored-Medium-{}",nonce.iter().map(|b|format!("{b:02x}")).collect::<String>())}
 pub fn server(nonce:[u8;16])->Result<Self>{let security=Security::owner()?;let sa=security.attributes(false);let handle=Handle::new(unsafe{CreateNamedPipeW(wide(Self::name(nonce)).as_ptr(),PIPE_ACCESS_DUPLEX|FILE_FLAG_FIRST_PIPE_INSTANCE|FILE_FLAG_OVERLAPPED,PIPE_TYPE_MESSAGE|PIPE_READMODE_MESSAGE|PIPE_REJECT_REMOTE_CLIENTS,1,256,256,1000,&sa)})?;Ok(Self{handle,server:true})}
 pub fn client(nonce:[u8;16])->Result<Self>{let name=wide(Self::name(nonce));let end=Instant::now()+Duration::from_secs(5);loop{let handle=unsafe{CreateFileW(name.as_ptr(),GENERIC_READ|GENERIC_WRITE,0,ptr::null(),OPEN_EXISTING,FILE_FLAG_OVERLAPPED|SECURITY_SQOS_PRESENT|SECURITY_IDENTIFICATION,ptr::null_mut())};if handle!=INVALID_HANDLE_VALUE{let pipe=Self{handle:Handle::new(handle)?,server:false};let mode=PIPE_READMODE_MESSAGE;yes(unsafe{SetNamedPipeHandleState(pipe.handle.0,&mode,ptr::null(),ptr::null())})?;return Ok(pipe)}let code=unsafe{GetLastError()};if code!=ERROR_PIPE_BUSY||Instant::now()>=end{return Err("private_pipe_open_failed")}unsafe{WaitNamedPipeW(name.as_ptr(),50)};}}
 fn operation(&self)->Result<Box<Operation>>{let event=Handle::new(unsafe{CreateEventW(ptr::null(),1,0,ptr::null())})?;let mut copy=ptr::null_mut();yes(unsafe{DuplicateHandle(GetCurrentProcess(),self.handle.0,GetCurrentProcess(),&mut copy,0,0,DUPLICATE_SAME_ACCESS)})?;let pipe=Handle::new(copy)?;let mut overlapped=OVERLAPPED::default();overlapped.hEvent=event.0;let operation=Box::new(Operation{overlapped,buffer:[0;256],_event:event,pipe});#[cfg(test)]{OPERATIONS.fetch_add(1,std::sync::atomic::Ordering::SeqCst);}Ok(operation)}
 fn complete(op:Box<Operation>,immediate:i32,error:u32,timeout_ms:u32)->Result<(Box<Operation>,u32)>{
  if immediate==0{if error!=ERROR_IO_PENDING{return Err("private_pipe_operation_failed")}
   if unsafe{WaitForSingleObject(op.overlapped.hEvent,timeout_ms)}!=WAIT_OBJECT_0{
    unsafe{CancelIoEx(op.pipe.0,&op.overlapped)};
    if unsafe{WaitForSingleObject(op.overlapped.hEvent,1000)}!=WAIT_OBJECT_0{
     // Keep kernel-referenced memory/handles valid until process teardown.
     std::mem::forget(op);return Err("pipe_cancel_unverified")
    }
    let mut n=0;let completed=unsafe{GetOverlappedResult(op.pipe.0,&op.overlapped,&mut n,0)};let completion_error=if completed==0{unsafe{GetLastError()}}else{0};
    if completed==0&&matches!(completion_error,ERROR_IO_INCOMPLETE|ERROR_IO_PENDING){std::mem::forget(op);return Err("pipe_cancel_unverified")}
    return Err(if completed==0&&completion_error==ERROR_OPERATION_ABORTED{"private_pipe_deadline"}else if completed!=0{"private_pipe_deadline_completed"}else{"pipe_cancel_result_unverified"})
   }
  }
  let mut n=0;let completed=unsafe{GetOverlappedResult(op.pipe.0,&op.overlapped,&mut n,0)};let error=if completed==0{unsafe{GetLastError()}}else{0};
  if completed==0&&matches!(error,ERROR_IO_INCOMPLETE|ERROR_IO_PENDING){std::mem::forget(op);return Err("pipe_completion_unverified")}
  if completed==0{return Err("private_pipe_operation_failed")}Ok((op,n))
 }
 pub fn accept(&self)->Result<()>{self.accept_for(15000)}
 fn accept_for(&self,timeout_ms:u32)->Result<()>{if !(1..=15000).contains(&timeout_ms){return Err("pipe_wait_bound")}let mut op=self.operation()?;let result=unsafe{ConnectNamedPipe(op.pipe.0,&mut op.overlapped)};let error=if result==0{unsafe{GetLastError()}}else{0};if result==0&&error==ERROR_PIPE_CONNECTED{return Ok(())}Self::complete(op,result,error,timeout_ms).map(|_|())}
 pub fn peer_pid(&self)->Result<u32>{let mut pid=0;yes(unsafe{if self.server{GetNamedPipeClientProcessId(self.handle.0,&mut pid)}else{GetNamedPipeServerProcessId(self.handle.0,&mut pid)}})?;if pid==0{return Err("pipe_peer_required")}Ok(pid)}
 pub fn send(&self,frame:Frame)->Result<()>{Frame::decode(&frame.encode())?;let mut op=self.operation()?;op.buffer[..BYTES].copy_from_slice(&frame.encode());let result=unsafe{WriteFile(op.pipe.0,op.buffer.as_ptr(),BYTES as u32,ptr::null_mut(),&mut op.overlapped)};let error=if result==0{unsafe{GetLastError()}}else{0};let(_,n)=Self::complete(op,result,error,5000)?;if n!=BYTES as u32{return Err("pipe_frame_write_bound")}Ok(())}
 pub fn receive(&self)->Result<Frame>{self.receive_for(15000)}
 fn receive_for(&self,timeout_ms:u32)->Result<Frame>{if !(1..=15000).contains(&timeout_ms){return Err("pipe_wait_bound")}let mut op=self.operation()?;let result=unsafe{ReadFile(op.pipe.0,op.buffer.as_mut_ptr(),BYTES as u32,ptr::null_mut(),&mut op.overlapped)};let error=if result==0{unsafe{GetLastError()}}else{0};let(op,n)=Self::complete(op,result,error,timeout_ms)?;if n!=BYTES as u32{return Err("pipe_frame_read_bound")}Frame::decode(&op.buffer[..BYTES])}
}

#[cfg(test)]mod tests{
 use super::*;use crate::protocol::{Stamp,Tag,EMPTY};use std::sync::{Mutex,mpsc,atomic::Ordering};
 static SERIAL:Mutex<()>=Mutex::new(());
 fn random_nonce()->[u8;16]{let mut nonce=[0;16];assert_eq!(unsafe{windows_sys::Win32::Security::Cryptography::BCryptGenRandom(ptr::null_mut(),nonce.as_mut_ptr(),16,windows_sys::Win32::Security::Cryptography::BCRYPT_USE_SYSTEM_PREFERRED_RNG)},0);assert_ne!(nonce,[0;16]);nonce}
 fn pair()->(Pipe,Pipe,[u8;16]){let n=random_nonce();let server=Pipe::server(n).unwrap();let client=Pipe::client(n).unwrap();server.accept_for(1000).unwrap();assert_eq!(server.peer_pid().unwrap(),std::process::id());assert_eq!(client.peer_pid().unwrap(),std::process::id());(server,client,n)}
 fn handles()->u32{let mut count=0;assert_ne!(unsafe{GetProcessHandleCount(GetCurrentProcess(),&mut count)},0);count}
 struct Baseline{os:u32,created:u64,closed:u64}
 fn start()->Baseline{
  let token=crate::process::token_facts(unsafe{GetCurrentProcess()}).unwrap();println!("owned transport token elevated: {}",token.elevated);
  drop(pair());
  // The first pending I/O observed one process-wide handle allocation. Do not
  // count this as an owned leak-free proof; measure repeated controls afterward.
  fn prime(){let server=Pipe::server(random_nonce()).unwrap();assert_eq!(server.accept_for(10),Err("private_pipe_deadline"));drop(server);assert_eq!(OPERATIONS.load(Ordering::SeqCst),0);}
  prime();
  let(created,closed,alive,failed)=crate::process::handle_metrics();assert_eq!(alive,0);assert_eq!(failed,0);assert_eq!(created,closed);Baseline{os:handles(),created,closed}
 }
 fn finished(before:Baseline){assert_eq!(OPERATIONS.load(Ordering::SeqCst),0);let(created,closed,alive,failed)=crate::process::handle_metrics();assert_eq!(alive,0);assert_eq!(failed,0);assert_eq!(created-before.created,closed-before.closed);let after=handles();assert!(after<=before.os,"owned pipe handle count increased: {} -> {after}",before.os);}
 fn raw(pipe:&Pipe,bytes:&[u8])->Result<()>{if bytes.len()>256{return Err("test_frame_bound")}let mut op=pipe.operation()?;op.buffer[..bytes.len()].copy_from_slice(bytes);let result=unsafe{WriteFile(op.pipe.0,op.buffer.as_ptr(),bytes.len()as u32,ptr::null_mut(),&mut op.overlapped)};let error=if result==0{unsafe{GetLastError()}}else{0};let(_,written)=Pipe::complete(op,result,error,1000)?;if written!=bytes.len()as u32{return Err("test_write_incomplete")}Ok(())}
 #[test]fn private_pipe_roundtrip_waits_for_delayed_writer_and_checks_os_peer(){
  let _serial=SERIAL.lock().unwrap();let before=start();let(server,client,nonce)=pair();let hello=Frame::new(Tag::Hello,nonce,0,Stamp{pid:11,created:22},0);let ready=Frame::new(Tag::Ready,nonce,1,EMPTY,4);
  let(start_tx,start_rx)=mpsc::channel();let(done_tx,done_rx)=mpsc::channel();
  let reader=std::thread::Builder::new().name("owned-private-pipe-reader".into()).spawn(move||{let _=start_tx.send(());let result=(||{let received=server.receive_for(1000)?;if received!=hello{return Err("test_frame_mismatch")}server.send(ready)})();let _=done_tx.send(result);}).unwrap();
  let began=start_rx.recv_timeout(Duration::from_secs(1)).is_ok();let waited=matches!(done_rx.recv_timeout(Duration::from_millis(40)),Err(mpsc::RecvTimeoutError::Timeout));let sent=client.send(hello);let reply=client.receive_for(1000);drop(client);let done=done_rx.recv_timeout(Duration::from_secs(2));let joined=reader.join();
  finished(before);assert!(began&&waited);assert_eq!(sent,Ok(()));assert_eq!(reply,Ok(ready));assert!(matches!(done,Ok(Ok(()))));assert!(joined.is_ok());
 }
 #[test]fn private_pipe_rejects_actual_short_extra_and_malformed_messages(){
  let _serial=SERIAL.lock().unwrap();let before=start();
  for case in 0..5{let(server,client,n)=pair();let valid=Frame::new(Tag::Hello,n,0,Stamp{pid:1,created:2},0).encode();let mut bytes=valid.to_vec();match case{0=>bytes.truncate(1),1=>bytes.truncate(63),2=>bytes.push(0),3=>bytes[5]=255,4=>bytes[44]=1,_=>{}}let sent=raw(&client,&bytes);let received=server.receive_for(1000);drop((server,client));assert_eq!(sent,Ok(()));assert!(received.is_err());}
  finished(before);
 }
 #[test]fn private_pipe_eof_rejects_without_pending_operation_or_handle_leak(){let _serial=SERIAL.lock().unwrap();let before=start();let(server,client,_)=pair();drop(client);let result=server.receive_for(1000);drop(server);finished(before);assert_eq!(result,Err("private_pipe_operation_failed"));}
 #[test]fn private_pipe_read_cancel_is_confirmed_and_next_message_is_not_stolen(){
  let _serial=SERIAL.lock().unwrap();let before=start();let(server,client,n)=pair();let began=Instant::now();let cancelled=server.receive_for(40);let elapsed=began.elapsed();let invalid=server.receive_for(0);let expected=Frame::new(Tag::Hello,n,0,Stamp{pid:1,created:2},0);let sent=client.send(expected);let next=server.receive_for(1000);drop((server,client));finished(before);assert_eq!(cancelled,Err("private_pipe_deadline"));assert!(elapsed<Duration::from_secs(3));assert_eq!(invalid,Err("pipe_wait_bound"));assert_eq!(sent,Ok(()));assert_eq!(next,Ok(expected));
 }
 #[test]fn private_pipe_connect_cancel_is_confirmed_before_resources_drop(){let _serial=SERIAL.lock().unwrap();let before=start();let server=Pipe::server(random_nonce()).unwrap();let invalid=server.accept_for(0);let began=Instant::now();let cancelled=server.accept_for(40);let elapsed=began.elapsed();drop(server);finished(before);assert_eq!(invalid,Err("pipe_wait_bound"));assert_eq!(cancelled,Err("private_pipe_deadline"));assert!(elapsed<Duration::from_secs(3));}
}
