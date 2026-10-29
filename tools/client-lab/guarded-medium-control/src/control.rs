use crate::{Result,Prepared,Scenario,ipc::Pipe,process::{self,Handle,Image,Peer},protocol::{Frame,Launcher,Stamp,Tag,EMPTY,Recovery,GuardRetention}};
use std::{fs::File,io::Write,ptr,time::{Duration,Instant}};
use windows_sys::Win32::{Security::Cryptography::*,System::{LibraryLoader::*,Threading::*},UI::{Shell::*,WindowsAndMessaging::SW_HIDE}};
fn require_supported_os()->Result<()>{let mut version=windows_sys::Win32::System::SystemInformation::OSVERSIONINFOW::default();version.dwOSVersionInfoSize=std::mem::size_of_val(&version)as u32;if unsafe{windows_sys::Wdk::System::SystemServices::RtlGetVersion(&mut version)}!=0||!crate::pe::supported_platform(version.dwMajorVersion,version.dwMinorVersion,version.dwBuildNumber){return Err("supported_windows_build_required")}Ok(())}
fn hard_deadline()->Result<()>{std::thread::Builder::new().name("owned-control-deadline".into()).spawn(||{std::thread::sleep(Duration::from_secs(130));std::process::exit(124)}).map_err(|_|"deadline_thread_unavailable")?;Ok(())}
fn nonce()->Result<[u8;16]>{let mut n=[0;16];if unsafe{BCryptGenRandom(ptr::null_mut(),n.as_mut_ptr(),16,BCRYPT_USE_SYSTEM_PREFERRED_RNG)}<0||n==[0;16]{return Err("nonce_unavailable")}Ok(n)}
pub fn nonce_text(n:[u8;16])->String{n.iter().map(|b|format!("{b:02x}")).collect()}
pub fn parse_nonce(s:&str)->Result<[u8;16]>{if s.len()!=32||!s.bytes().all(|b|b.is_ascii_hexdigit()){return Err("closed_nonce_required")}let mut n=[0;16];for(i,b)in n.iter_mut().enumerate(){*b=u8::from_str_radix(&s[i*2..i*2+2],16).map_err(|_|"closed_nonce_required")?;}if n==[0;16]{return Err("nonce_unavailable")}Ok(n)}
fn pin_text(n:[u8;32])->String{n.iter().map(|b|format!("{b:02x}")).collect()}
fn own_output()->Result<(File,Vec<Handle>)>{let(dir,locks)=process::private_directory()?;let mut file=std::fs::OpenOptions::new().create_new(true).write(true).open(dir.join("phase1.json")).map_err(|_|"private_output_create_failed")?;file.write_all(b"{\"ok\":false,\"status\":\"pending\",\"executionReady\":false,\"targetNetworkVerified\":false}").and_then(|_|file.sync_all()).map_err(|_|"private_output_write_failed")?;Ok((file,locks))}
fn report(file:&mut File,result:&Result<()>,guard:bool,stage:&'static str)->Result<()>{use std::io::{Seek,SeekFrom};let text=match result{Ok(())=>format!("{{\"ok\":true,\"scope\":\"authored_lifecycle_only\",\"stage\":\"{stage}\",\"guard\":{guard},\"cleanupVerified\":true,\"targetNetworkVerified\":false,\"originalClientTested\":false,\"executionReady\":false}}"),Err(reason)=>format!("{{\"ok\":false,\"error\":\"{reason}\",\"stage\":\"{stage}\",\"cleanupVerified\":false,\"targetNetworkVerified\":false,\"originalClientTested\":false,\"executionReady\":false}}")};file.seek(SeekFrom::Start(0)).and_then(|_|file.write_all(text.as_bytes())).and_then(|_|file.set_len(text.len()as u64)).and_then(|_|file.sync_all()).map_err(|_|"private_output_write_failed")}
fn wait_exit(peer:&Peer,until:Instant)->Result<()>{while Instant::now()<until{if process::exited(peer)?{return Ok(())}std::thread::sleep(Duration::from_millis(5));}Err("retained_exit_unverified")}
fn peer_on_pipe(pipe:&Pipe,expected:&Peer)->Result<()>{if pipe.peer_pid()?!=expected.stamp.pid||process::exited(expected)?{return Err("os_pipe_peer_refused")}Ok(())}
/// Explicit role only. This is the sole normal-UAC call; no alternate targets.
pub fn launcher(launcher_pin:[u8;32],worker_pin:[u8;32],guard_pin:[u8;32])->Result<()>{
 require_supported_os()?;process::non_admin()?;let owner=process::token_facts(unsafe{GetCurrentProcess()})?;if owner.integrity!=8192{return Err("medium_integrity_required")}
 let(_,launcher_image,_launcher_locks)=process::fixed_image(Image::Launcher,launcher_pin)?;process::image_matches(unsafe{GetCurrentProcess()},&launcher_image)?;
 let(_worker, _worker_image,_worker_locks)=process::fixed_image(Image::Worker,worker_pin)?;
 let(guard_path,guard_image,_guard_locks)=process::fixed_image(Image::Guard,guard_pin)?;process::verify_guard_load_flags(&guard_image)?;
 let n=nonce()?;let pipe=Pipe::server(n)?;let stamp=process::process_stamp(unsafe{GetCurrentProcess()})?;let(mut output,_output_locks)=own_output()?;
 let result=(||{
  let parameters=process::wide(format!("--one-shot {} {} {} {} {} {}",nonce_text(n),stamp.pid,stamp.created,pin_text(launcher_pin),pin_text(worker_pin),pin_text(guard_pin)));
  let verb=process::wide("runas");let path=process::wide(&guard_path);let directory=process::wide(guard_path.parent().ok_or("guard_parent_required")?);
  let mut info=SHELLEXECUTEINFOW::default();info.cbSize=std::mem::size_of_val(&info)as u32;info.fMask=SEE_MASK_NOCLOSEPROCESS|SEE_MASK_NOASYNC;info.lpVerb=verb.as_ptr();info.lpFile=path.as_ptr();info.lpParameters=parameters.as_ptr();info.lpDirectory=directory.as_ptr();info.nShow=SW_HIDE;
  process::yes(unsafe{ShellExecuteExW(&mut info)})?;
  let retained=Handle::new(info.hProcess)?;hard_deadline()?;let helper_stamp=process::process_stamp(retained.0)?;let helper=process::verify_peer(retained,helper_stamp,&guard_image,&owner,true)?;
  pipe.accept()?;peer_on_pipe(&pipe,&helper)?;let mut session=Launcher::new(n,helper.stamp)?;session.hello(pipe.receive()?)?;session.ready(pipe.receive()?)?;peer_on_pipe(&pipe,&helper)?;
  // No target exists before the authenticated live-readback Ready receipt.
  let mut target=match Prepared::new(worker_pin,Scenario::Marker){Ok(target)=>target,Err(reason)=>{let _=pipe.send(session.cancel_without_target()?);return Err(reason)}};let target_stamp=target.stamp()?;pipe.send(session.register(target_stamp)?)?;
  peer_on_pipe(&pipe,&helper)?;session.ack(pipe.receive()?)?;session.resume_once()?;target.resume()?;
  let receipt=target.finish(Duration::from_secs(3))?;
  let good=receipt.resumed&&!receipt.timed_out&&receipt.before_cleanup==Some(0)&&receipt.exit_code==Some(0)&&receipt.job_empty&&receipt.stdout.eof&&receipt.stderr.eof&&receipt.stdout.expected;
  pipe.send(session.done(good)?)?;peer_on_pipe(&pipe,&helper)?;session.released(pipe.receive()?)?;wait_exit(&helper,Instant::now()+Duration::from_secs(3))?;
  let mut code=0;process::yes(unsafe{GetExitCodeProcess(helper.handle.0,&mut code)})?;if code!=0{return Err("guard_exit_not_success")}Ok(())
 })();report(&mut output,&result,false,"medium_control")?;result
}
/// Source-only until reviewed and explicitly invoked with normal Windows consent.
pub fn guard(n:[u8;16],parent:Stamp,launcher_pin:[u8;32],worker_pin:[u8;32],guard_pin:[u8;32])->Result<()>{
 require_supported_os()?;process::yes(unsafe{SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_SYSTEM32)})?;hard_deadline()?;
 let owner=process::token_facts(unsafe{GetCurrentProcess()})?;if !owner.elevated||owner.integrity!=12288{return Err("high_integrity_required")}
 let(_,guard_image,_guard_locks)=process::fixed_image(Image::Guard,guard_pin)?;process::verify_guard_load_flags(&guard_image)?;process::image_matches(unsafe{GetCurrentProcess()},&guard_image)?;
 // Caller admission precedes every high diagnostic directory/file creation.
 let(_,launcher_image,_launcher_locks)=process::fixed_image(Image::Launcher,launcher_pin)?;
 let launcher=process::open_peer(parent,&launcher_image,&owner,false)?;
 let pipe=Pipe::client(n)?;peer_on_pipe(&pipe,&launcher)?;
 let(mut output,_output_locks)=own_output()?;let mut stage="admitted";
 let result=guard_admitted(n,worker_pin,&owner,&launcher,&pipe,&mut stage);
 report(&mut output,&result,true,stage)?;result
}
fn guard_admitted(n:[u8;16],worker_pin:[u8;32],owner:&process::TokenFacts,launcher:&Peer,pipe:&Pipe,stage:&mut &'static str)->Result<()>{
 *stage="worker_image";
 let(worker_path,worker_image,_worker_locks)=process::fixed_image(Image::Worker,worker_pin)?;
 *stage="hello_send";let own=process::process_stamp(unsafe{GetCurrentProcess()})?;pipe.send(Frame::new(Tag::Hello,n,0,own,0))?;
 *stage="filter_create";
 let budget=metallic_external_guard_fixture::Budget::default();let mut api=crate::wfp::Native::new(budget.clone(),worker_path)?;
 *stage="filter_install";
 let rules=metallic_external_guard_fixture::install(&mut api)?;let mut target:Option<Peer>=None;
 let result=(||{
  *stage="live_ready";
  peer_on_pipe(pipe,launcher)?;pipe.send(Frame::new(Tag::Ready,n,1,EMPTY,4))?;
  let registration=pipe.receive()?;peer_on_pipe(pipe,launcher)?;
  if registration.tag==Tag::Cancel{registration.expect(n,Tag::Cancel,2,EMPTY)?;return Err("cancelled_before_target")}
  registration.expect(n,Tag::Register,2,registration.stamp)?;
  *stage="target_registration";
  let verified=process::open_peer(registration.stamp,&worker_image,owner,false)?;
  if verified.stamp.created<launcher.stamp.created{return Err("target_creation_order_refused")}
  target=Some(verified);
  // The real query/synchronize handle is retained before Ack can permit resume.
  *stage="target_ack";
  pipe.send(Frame::new(Tag::Ack,n,3,target.as_ref().unwrap().stamp,0))?;
  *stage="target_done";
  let done=pipe.receive()?;peer_on_pipe(pipe,launcher)?;done.expect(n,Tag::Done,4,target.as_ref().unwrap().stamp)?;
  Ok(())
 })();
 // Any post-Ready failure retains guards while the known target (or the
 // launcher if registration was lost) can still be alive. Never accept a PID-only claim.
 let failure_stage=*stage;*stage="stopwait";
 let mut retention=GuardRetention::new(target.as_ref().map(|target|target.stamp));
 let (waited,subject)=if let Some(target)=target.as_ref(){(target,Recovery::WaitTargetExit)}else{(launcher,Recovery::WaitLauncherExit)};
 loop{if retention.observe(subject,process::exited(waited).ok()){break}std::thread::sleep(Duration::from_millis(5));}
 // Only the independent 130-second hard exit may end an unverified wait.
 // It leaves failure/pending evidence and is not atomic lifetime containment.
 *stage="target_exit";
 let target_status=(||{if result.is_ok(){let mut code=0;process::yes(unsafe{GetExitCodeProcess(waited.handle.0,&mut code)})?;if code!=0{return Err("target_exit_not_zero")}}Ok(())})();
 *stage="filter_cleanup";
 let removed=metallic_external_guard_fixture::cleanup(&mut api,&rules,true);
 let final_result=result.and(target_status).and(removed);
 if result.is_err(){*stage=failure_stage;}else if target_status.is_err(){*stage="target_exit";}
 if final_result.is_ok(){*stage="released_send";pipe.send(Frame::new(Tag::Released,n,5,target.as_ref().ok_or("target_required")?.stamp,0))?;}
 if final_result.is_ok(){*stage="complete";}final_result
}
