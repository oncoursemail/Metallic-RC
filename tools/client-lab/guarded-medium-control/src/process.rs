use crate::{environment_block,Result,Scenario,WORKER};
use std::{ffi::c_void,fs::File,io::{Read,Write},os::windows::{ffi::OsStrExt,io::{AsRawHandle,FromRawHandle}},path::{Path,PathBuf},ptr,sync::{Arc,atomic::{AtomicBool,AtomicU64,Ordering},mpsc},thread,time::{Duration,Instant,SystemTime,UNIX_EPOCH}};
use windows_sys::Win32::{Foundation::*,Security::{*,Authorization::*,Cryptography::*},Storage::FileSystem::*,System::{Threading::*,JobObjects::*,Pipes::*}};

pub(crate) fn wide(s:impl AsRef<std::ffi::OsStr>)->Vec<u16>{s.as_ref().encode_wide().chain(Some(0)).collect()}
pub(crate) fn yes(value:i32)->Result<()>{if value==0{Err("owned_win32_operation_failed")}else{Ok(())}}
pub(crate) fn non_admin()->Result<()>{let mut h=ptr::null_mut();yes(unsafe{OpenProcessToken(GetCurrentProcess(),TOKEN_QUERY,&mut h)})?;let token=Handle::new(h)?;let mut info=TOKEN_ELEVATION::default();let mut size=0;yes(unsafe{GetTokenInformation(token.0,TokenElevation,(&mut info as *mut TOKEN_ELEVATION).cast(),std::mem::size_of_val(&info)as u32,&mut size)})?;if info.TokenIsElevated!=0{Err("non_admin_fixture_required")}else{Ok(())}}
pub struct Handle(pub(crate) HANDLE);
unsafe impl Send for Handle{}
#[cfg(test)]static HANDLE_CREATED:AtomicU64=AtomicU64::new(0);
#[cfg(test)]static HANDLE_CLOSED:AtomicU64=AtomicU64::new(0);
#[cfg(test)]static HANDLE_ALIVE:AtomicU64=AtomicU64::new(0);
#[cfg(test)]static HANDLE_CLOSE_FAILURE:AtomicU64=AtomicU64::new(0);
#[cfg(test)]pub(crate) fn handle_metrics()->(u64,u64,u64,u64){(HANDLE_CREATED.load(Ordering::SeqCst),HANDLE_CLOSED.load(Ordering::SeqCst),HANDLE_ALIVE.load(Ordering::SeqCst),HANDLE_CLOSE_FAILURE.load(Ordering::SeqCst))}
impl Handle{
 fn owned(h:HANDLE)->Self{#[cfg(test)]{HANDLE_CREATED.fetch_add(1,Ordering::SeqCst);HANDLE_ALIVE.fetch_add(1,Ordering::SeqCst);}Self(h)}
 pub(crate) fn new(h:HANDLE)->Result<Self>{if h.is_null()||h==INVALID_HANDLE_VALUE{Err("owned_handle_failed")}else{Ok(Self::owned(h))}}
}
impl Drop for Handle{fn drop(&mut self){let _closed=unsafe{CloseHandle(self.0)};#[cfg(test)]{if _closed!=0{HANDLE_CLOSED.fetch_add(1,Ordering::SeqCst);HANDLE_ALIVE.fetch_sub(1,Ordering::SeqCst);}else{HANDLE_CLOSE_FAILURE.fetch_add(1,Ordering::SeqCst);}}}}

pub(crate) struct Security(*mut c_void);
impl Security{
    pub(crate) fn owner()->Result<Self>{
        let mut token=ptr::null_mut();yes(unsafe{OpenProcessToken(GetCurrentProcess(),TOKEN_QUERY,&mut token)})?;let token=Handle::new(token)?;
        let mut size=0;unsafe{GetTokenInformation(token.0,TokenUser,ptr::null_mut(),0,&mut size)};
        if size==0||size>8192{return Err("owner_token_bound")}
        let mut buffer=vec![0u64;(size as usize+7)/8];yes(unsafe{GetTokenInformation(token.0,TokenUser,buffer.as_mut_ptr().cast(),size,&mut size)})?;
        let user=unsafe{&*(buffer.as_ptr() as *const TOKEN_USER)};let mut sid=ptr::null_mut();yes(unsafe{ConvertSidToStringSidW(user.User.Sid,&mut sid)})?;
        let string=(||{let mut n=0;while n<256&&unsafe{*sid.add(n)}!=0{n+=1}if n==256{return Err("owner_sid_bound")}Ok(String::from_utf16_lossy(unsafe{std::slice::from_raw_parts(sid,n)}))})();unsafe{LocalFree(sid.cast())};
        let text=wide(format!("D:P(A;OICI;FA;;;{})",string?));let mut sd=ptr::null_mut();yes(unsafe{ConvertStringSecurityDescriptorToSecurityDescriptorW(text.as_ptr(),1,&mut sd,ptr::null_mut())})?;Ok(Self(sd))
    }
    pub(crate) fn attributes(&self,inherit:bool)->SECURITY_ATTRIBUTES{SECURITY_ATTRIBUTES{nLength:std::mem::size_of::<SECURITY_ATTRIBUTES>()as u32,lpSecurityDescriptor:self.0,bInheritHandle:inherit as i32}}
}
impl Drop for Security{fn drop(&mut self){unsafe{LocalFree(self.0)};}}

fn lock_directory(path:&Path)->Result<Handle>{
    let h=Handle::new(unsafe{CreateFileW(wide(path).as_ptr(),FILE_READ_ATTRIBUTES|FILE_LIST_DIRECTORY,FILE_SHARE_READ,ptr::null(),OPEN_EXISTING,FILE_FLAG_BACKUP_SEMANTICS|FILE_FLAG_OPEN_REPARSE_POINT,ptr::null_mut())})?;
    let mut data=BY_HANDLE_FILE_INFORMATION::default();yes(unsafe{GetFileInformationByHandle(h.0,&mut data)})?;
    if data.dwFileAttributes&FILE_ATTRIBUTE_REPARSE_POINT!=0||data.dwFileAttributes&FILE_ATTRIBUTE_DIRECTORY==0{return Err("directory_identity_refused")}Ok(h)
}
pub(crate) fn lock_ancestors(path:&Path)->Result<Vec<Handle>>{
    use std::path::{Component,Prefix};
    if !path.is_absolute()||!matches!(path.components().next(),Some(Component::Prefix(p))if matches!(p.kind(),Prefix::Disk(_)|Prefix::VerbatimDisk(_)))||path.components().any(|c|matches!(c,Component::ParentDir)){return Err("local_absolute_directory_required")}
    path.ancestors().collect::<Vec<_>>().into_iter().rev().map(lock_directory).collect()
}
pub(crate) fn private_directory()->Result<(PathBuf,Vec<Handle>)>{
    let local=std::env::var_os("LOCALAPPDATA").ok_or("localappdata_required")?;let base=PathBuf::from(local);
    if !base.is_absolute(){return Err("absolute_localappdata_required")}
    let security=Security::owner()?;let sa=security.attributes(false);
    let mut locks=lock_ancestors(&base)?;let mut path=base;
    for name in ["Metallic-RC","guarded-medium-control-checks"]{
        path.push(name);let w=wide(&path);
        if unsafe{CreateDirectoryW(w.as_ptr(),&sa)}==0&&unsafe{GetLastError()}!=ERROR_ALREADY_EXISTS{return Err("private_parent_create_failed")}
        let attrs=unsafe{GetFileAttributesW(w.as_ptr())};if attrs==INVALID_FILE_ATTRIBUTES||attrs&FILE_ATTRIBUTE_REPARSE_POINT!=0||attrs&FILE_ATTRIBUTE_DIRECTORY==0{return Err("private_parent_not_directory")}
        locks.push(lock_directory(&path)?);
    }
    let nonce=SystemTime::now().duration_since(UNIX_EPOCH).map_err(|_|"clock_failed")?.as_nanos();path.push(format!("owned-{}-{nonce}",std::process::id()));
    yes(unsafe{CreateDirectoryW(wide(&path).as_ptr(),&sa)})?;locks.push(lock_directory(&path)?);Ok((path,locks))
}
pub(crate) fn hash(bytes:&[u8])->Result<[u8;32]>{
    let mut h=ptr::null_mut();if unsafe{BCryptOpenAlgorithmProvider(&mut h,BCRYPT_SHA256_ALGORITHM,ptr::null(),0)}<0{return Err("sha256_provider_failed")}
    let mut output=[0;32];let result=unsafe{BCryptHash(h,ptr::null(),0,bytes.as_ptr(),bytes.len()as u32,output.as_mut_ptr(),32)};unsafe{BCryptCloseAlgorithmProvider(h,0)};
    if result<0{Err("sha256_failed")}else{Ok(output)}
}
pub(crate) fn file_info(file:&File)->Result<(u32,u32,u32)>{let mut data=BY_HANDLE_FILE_INFORMATION::default();yes(unsafe{GetFileInformationByHandle(file.as_raw_handle()as HANDLE,&mut data)})?;if data.dwFileAttributes&FILE_ATTRIBUTE_REPARSE_POINT!=0{return Err("worker_reparse_refused")}Ok((data.dwVolumeSerialNumber,data.nFileIndexHigh,data.nFileIndexLow))}
fn lock_image(path:&Path)->Result<File>{
    let h=unsafe{CreateFileW(wide(path).as_ptr(),GENERIC_READ,FILE_SHARE_READ,ptr::null(),OPEN_EXISTING,FILE_ATTRIBUTE_NORMAL|FILE_FLAG_OPEN_REPARSE_POINT,ptr::null_mut())};
    if h==INVALID_HANDLE_VALUE{return Err("worker_read_lock_failed")}Ok(unsafe{File::from_raw_handle(h as _)})
}
fn worker_image(pin:[u8;32])->Result<(PathBuf,File,Vec<Handle>)>{fixed_image(Image::Worker,pin)}
pub(crate) fn fixed_image(kind:Image,pin:[u8;32])->Result<(PathBuf,File,Vec<Handle>)>{
    let path=std::env::current_exe().map_err(|_|"checker_image_unavailable")?.parent().ok_or("checker_parent_unavailable")?.join(kind.name());
    let directories=lock_ancestors(path.parent().ok_or("worker_parent_missing")?)?;
    let mut file=lock_image(&path)?;file_info(&file)?;let mut bytes=Vec::new();std::io::Read::by_ref(&mut file).take(16*1024*1024+1).read_to_end(&mut bytes).map_err(|_|"worker_hash_read_failed")?;
    if bytes.len()>16*1024*1024||hash(&bytes)?!=pin{return Err("worker_pin_mismatch")}Ok((path,file,directories))
}
pub(crate) fn image_matches(process:HANDLE,pin:&File)->Result<()>{
    let mut text=vec![0u16;32768];let mut size=text.len()as u32;yes(unsafe{QueryFullProcessImageNameW(process,0,text.as_mut_ptr(),&mut size)})?;
    let path=PathBuf::from(std::ffi::OsString::from_wide(&text[..size as usize]));let actual=lock_image(&path)?;
    let actual_id=file_info(&actual)?;let expected_id=file_info(pin)?;if actual_id!=expected_id{return Err("retained_worker_identity_mismatch")}Ok(())
}

struct Attributes{storage:Vec<usize>,list:LPPROC_THREAD_ATTRIBUTE_LIST}
impl Attributes{
    fn new(handles:&[HANDLE])->Result<Self>{
        let mut size=0;unsafe{InitializeProcThreadAttributeList(ptr::null_mut(),1,0,&mut size)};if size==0||size>65536{return Err("attribute_bound")}
        let mut storage=vec![0usize;(size+std::mem::size_of::<usize>()-1)/std::mem::size_of::<usize>()];let list=storage.as_mut_ptr().cast();
        yes(unsafe{InitializeProcThreadAttributeList(list,1,0,&mut size)})?;let result=Self{storage,list};
        yes(unsafe{UpdateProcThreadAttribute(list,0,PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,handles.as_ptr().cast(),std::mem::size_of_val(handles),ptr::null_mut(),ptr::null())})?;Ok(result)
    }
}
impl Drop for Attributes{fn drop(&mut self){unsafe{DeleteProcThreadAttributeList(self.list)};let _=&self.storage;}}
fn pipe()->Result<(File,Handle)>{
    let(mut read,mut write)=(ptr::null_mut(),ptr::null_mut());let sa=SECURITY_ATTRIBUTES{nLength:std::mem::size_of::<SECURITY_ATTRIBUTES>()as u32,lpSecurityDescriptor:ptr::null_mut(),bInheritHandle:1};
    yes(unsafe{CreatePipe(&mut read,&mut write,&sa,0)})?;let read=unsafe{File::from_raw_handle(read as _)};let write=Handle::new(write)?;
    yes(unsafe{SetHandleInformation(read.as_raw_handle()as HANDLE,HANDLE_FLAG_INHERIT,0)})?;let mut flags=0;yes(unsafe{GetHandleInformation(read.as_raw_handle()as HANDLE,&mut flags)})?;if flags&HANDLE_FLAG_INHERIT!=0{return Err("parent_pipe_inheritance_refused")}Ok((read,write))
}

#[derive(Debug)]pub struct Capture{pub bytes:u64,pub prefix_bytes:usize,pub sha256:[u8;32],pub eof:bool,pub expected:bool}
#[derive(Default)]struct DrainMetrics{started:AtomicU64,active:AtomicU64,eof:AtomicU64,bytes:AtomicU64}
struct ActiveDrain(Arc<DrainMetrics>);
impl Drop for ActiveDrain{fn drop(&mut self){self.0.active.fetch_sub(1,Ordering::SeqCst);}}
struct Drain{count:Arc<AtomicU64>,stop:Arc<AtomicBool>,result:mpsc::Receiver<Result<Capture>>,thread:Option<thread::JoinHandle<()>>}
impl Drain{
    fn start(mut file:File,expected:Vec<u8>,metrics:Arc<DrainMetrics>,injected_failure:bool)->Result<Self>{
        let count=Arc::new(AtomicU64::new(0));let stop=Arc::new(AtomicBool::new(false));let(tx,rx)=mpsc::channel();let(c,s)=(count.clone(),stop.clone());
        let operation=move||{
            metrics.started.fetch_add(1,Ordering::SeqCst);metrics.active.fetch_add(1,Ordering::SeqCst);let _active=ActiveDrain(metrics.clone());
            let result=(||{let mut prefix=Vec::new();let mut eof=false;let mut buffer=[0;4096];
                loop{let mut available=0;let ok=unsafe{PeekNamedPipe(file.as_raw_handle()as HANDLE,ptr::null_mut(),0,ptr::null_mut(),&mut available,ptr::null_mut())};
                    if ok==0{if unsafe{GetLastError()}==ERROR_BROKEN_PIPE{eof=true;break}else{return Err("output_peek_failed")}}
                    if available==0{if s.load(Ordering::SeqCst){break}thread::sleep(Duration::from_millis(2));continue}
                    let n=file.read(&mut buffer[..(available as usize).min(4096)]).map_err(|_|"output_read_failed")?;if n==0{eof=true;break}
                    c.fetch_add(n as u64,Ordering::SeqCst);metrics.bytes.fetch_add(n as u64,Ordering::SeqCst);let keep=n.min(65536-prefix.len());prefix.extend_from_slice(&buffer[..keep]);
                }
                Ok(Capture{bytes:c.load(Ordering::SeqCst),prefix_bytes:prefix.len(),sha256:hash(&prefix)?,eof,expected:prefix==expected})
            })();if matches!(&result,Ok(capture)if capture.eof){metrics.eof.fetch_add(1,Ordering::SeqCst);}let _=tx.send(result);
        };
        let spawned=if injected_failure{Err(std::io::Error::from(std::io::ErrorKind::Other))}else{thread::Builder::new().name("owned-output-drain".into()).spawn(operation)};
        let thread=spawned.map_err(|_|"output_drain_start_failed")?;Ok(Self{count,stop,result:rx,thread:Some(thread)})
    }
    fn finish(mut self)->Result<Capture>{
        let result=self.result.recv_timeout(Duration::from_secs(2));
        if result.is_err(){self.stop.store(true,Ordering::SeqCst);}
        let result=match result{Ok(value)=>value,Err(_)=>self.result.recv_timeout(Duration::from_secs(1)).map_err(|_|"output_drain_deadline")?};
        if let Some(thread)=self.thread.take(){thread.join().map_err(|_|"output_drain_panic")?;}result
    }
}
impl Drop for Drain{fn drop(&mut self){self.stop.store(true,Ordering::SeqCst);}}

pub struct Receipt{pub resumed:bool,pub timed_out:bool,pub before_cleanup:Option<u32>,pub exit_code:Option<u32>,pub job_empty:bool,pub stdout:Capture,pub stderr:Capture,pub total_processes:u32}
impl Receipt{
    pub fn json(&self)->String{
        let h=|bytes:[u8;32]|bytes.iter().map(|b|format!("{b:02x}")).collect::<String>();
        let exit=|v:Option<u32>|v.map(|n|n.to_string()).unwrap_or("null".into());
        format!("{{\"scope\":\"authored_sibling_worker_only\",\"resumed\":{},\"timedOut\":{},\"beforeCleanup\":{},\"exitCode\":{},\"jobEmpty\":{},\"totalProcesses\":{},\"stdoutBytes\":{},\"stdoutPrefixBytes\":{},\"stdoutPrefixSha256\":\"{}\",\"stdoutEof\":{},\"stderrBytes\":{},\"stderrPrefixBytes\":{},\"stderrPrefixSha256\":\"{}\",\"stderrEof\":{},\"originalClientTested\":false,\"executionReady\":false}}",self.resumed,self.timed_out,exit(self.before_cleanup),exit(self.exit_code),self.job_empty,self.total_processes,self.stdout.bytes,self.stdout.prefix_bytes,h(self.stdout.sha256),self.stdout.eof,self.stderr.bytes,self.stderr.prefix_bytes,h(self.stderr.sha256),self.stderr.eof)
    }
}
pub struct Prepared{process:Handle,thread:Handle,job:Handle,_pin:File,_image_directories:Vec<Handle>,_private_directories:Vec<Handle>,stdout:Option<Drain>,stderr:Option<Drain>,directory:PathBuf,resumed:bool,assigned:bool,cleaned:bool}
impl Prepared{
    pub fn new(pin:[u8;32],scenario:Scenario)->Result<Self>{
        Self::new_inner(pin,scenario,None,None)
    }
    fn new_inner(pin:[u8;32],scenario:Scenario,fault:Option<bool>,audit:Option<&mut StartupAudit>)->Result<Self>{
        non_admin()?;
        let metrics=audit.as_ref().map(|a|a.metrics.clone()).unwrap_or_else(||Arc::new(DrainMetrics::default()));
        let(path,image,image_directories)=worker_image(pin)?;let(directory,private_directories)=private_directory()?;
        let job=Handle::new(unsafe{CreateJobObjectW(ptr::null(),ptr::null())})?;let mut limits=JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();limits.BasicLimitInformation.LimitFlags=JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        yes(unsafe{SetInformationJobObject(job.0,JobObjectExtendedLimitInformation,(&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),std::mem::size_of_val(&limits)as u32)})?;
        let(out_read,out_write)=pipe()?;let(err_read,err_write)=pipe()?;
        let sa=SECURITY_ATTRIBUTES{nLength:std::mem::size_of::<SECURITY_ATTRIBUTES>()as u32,lpSecurityDescriptor:ptr::null_mut(),bInheritHandle:1};
        let stdin=Handle::new(unsafe{CreateFileW(wide("NUL").as_ptr(),GENERIC_READ,FILE_SHARE_READ|FILE_SHARE_WRITE,&sa,OPEN_EXISTING,FILE_ATTRIBUTE_NORMAL,ptr::null_mut())})?;
        let handles=[stdin.0,out_write.0,err_write.0];let attributes=Attributes::new(&handles)?;
        let rows=std::env::vars_os().filter(|(name,_)|name.to_str().map(|name|crate::ENVIRONMENT.iter().any(|key|key.eq_ignore_ascii_case(name))).unwrap_or(false)).map(|(name,value)|Ok((name.into_string().map_err(|_|"environment_unicode_required")?,value.into_string().map_err(|_|"environment_unicode_required")?))).collect::<Result<Vec<_>>>()?;
        let(mut env,count,drive)=environment_block(rows)?;
        let expected=match scenario{Scenario::Marker=>b"OWNED_MARKER\n".to_vec(),Scenario::Environment=>format!("ENV_ALLOWED={count};UNEXPECTED=0;SYSTEMDRIVE={}\n",drive as u8).into_bytes(),Scenario::Wait|Scenario::Grandchild=>b"OWNED_WAITING\n".to_vec(),_=>Vec::new()};
        let mut startup=STARTUPINFOEXW::default();startup.StartupInfo.cb=std::mem::size_of_val(&startup)as u32;startup.StartupInfo.dwFlags=STARTF_USESTDHANDLES;startup.StartupInfo.hStdInput=stdin.0;startup.StartupInfo.hStdOutput=out_write.0;startup.StartupInfo.hStdError=err_write.0;startup.lpAttributeList=attributes.list;
        let mut line=wide(format!("\"{}\" {}",path.to_str().ok_or("worker_path_unicode")?,scenario.argument()));let mut pi=PROCESS_INFORMATION::default();
        yes(unsafe{CreateProcessW(wide(&path).as_ptr(),line.as_mut_ptr(),ptr::null(),ptr::null(),1,CREATE_SUSPENDED|CREATE_NO_WINDOW|CREATE_UNICODE_ENVIRONMENT|EXTENDED_STARTUPINFO_PRESENT,env.as_mut_ptr().cast(),wide(path.parent().ok_or("worker_parent_missing")?).as_ptr(),&startup.StartupInfo,&mut pi)})?;
        // Take cleanup ownership before any fallible post-creation work.
        let mut prepared=Self{process:Handle::owned(pi.hProcess),thread:Handle::owned(pi.hThread),job,_pin:image,_image_directories:image_directories,_private_directories:private_directories,stdout:None,stderr:None,directory,resumed:false,assigned:false,cleaned:false};
        yes(unsafe{AssignProcessToJobObject(prepared.job.0,prepared.process.0)})?;prepared.assigned=true;
        let mut member=0;yes(unsafe{IsProcessInJob(prepared.process.0,prepared.job.0,&mut member)})?;if member==0{return Err("worker_job_admission_failed")}
        image_matches(prepared.process.0,&prepared._pin)?;
        if let Some(audit)=audit{audit.process=Some(duplicate(prepared.process.0,PROCESS_QUERY_LIMITED_INFORMATION|SYNCHRONIZE)?);audit.job=Some(duplicate(prepared.job.0,4)?);}
        drop(out_write);drop(err_write);drop(stdin);
        prepared.stdout=Some(Drain::start(out_read,expected,metrics.clone(),fault==Some(false))?);
        prepared.stderr=Some(Drain::start(err_read,Vec::new(),metrics,fault==Some(true))?);
        Ok(prepared)
    }
    pub fn stamp(&self)->Result<crate::protocol::Stamp>{process_stamp(self.process.0)}
    pub fn pid(&self)->u32{unsafe{GetProcessId(self.process.0)}}
    pub fn output_counts(&self)->(u64,u64){(self.stdout.as_ref().map(|d|d.count.load(Ordering::SeqCst)).unwrap_or(0),self.stderr.as_ref().map(|d|d.count.load(Ordering::SeqCst)).unwrap_or(0))}
    pub fn resume(&mut self)->Result<()>{if self.resumed||!self.assigned{return Err("resume_admission_refused")}if unsafe{ResumeThread(self.thread.0)}!=1{return Err("unexpected_suspend_count")}self.resumed=true;Ok(())}
    fn poll(&self)->Result<Option<u32>>{match unsafe{WaitForSingleObject(self.process.0,0)}{WAIT_TIMEOUT=>Ok(None),WAIT_OBJECT_0=>{let mut code=0;yes(unsafe{GetExitCodeProcess(self.process.0,&mut code)})?;Ok(Some(code))},_=>Err("retained_wait_failed")}}
    fn accounting(&self)->Result<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>{let mut data=JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();yes(unsafe{QueryInformationJobObject(self.job.0,JobObjectBasicAccountingInformation,(&mut data as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),std::mem::size_of_val(&data)as u32,ptr::null_mut())})?;Ok(data)}
    pub fn process_count(&self)->Result<u32>{Ok(self.accounting()?.TotalProcesses)}
    pub fn owned_pids(&self)->Result<Vec<u32>>{
        let mut data=[0usize;18];let mut returned=0;
        yes(unsafe{QueryInformationJobObject(self.job.0,JobObjectBasicProcessIdList,data.as_mut_ptr().cast(),std::mem::size_of_val(&data)as u32,&mut returned)})?;
        let list=unsafe{&*(data.as_ptr()as *const JOBOBJECT_BASIC_PROCESS_ID_LIST)};let n=list.NumberOfProcessIdsInList as usize;
        if n==0||n>16||list.NumberOfAssignedProcesses as usize!=n{return Err("owned_pid_list_bound")}
        let ids=unsafe{std::slice::from_raw_parts(list.ProcessIdList.as_ptr(),n)};ids.iter().map(|pid|u32::try_from(*pid).map_err(|_|"owned_pid_bound")).collect()
    }
    fn stop(&mut self)->Result<()>{
        if !self.assigned&&self.poll()?.is_none(){yes(unsafe{TerminateProcess(self.process.0,124)})?;}
        yes(unsafe{TerminateJobObject(self.job.0,124)})?;
        let end=Instant::now()+Duration::from_secs(2);
        while Instant::now()<end{if self.poll()?.is_some()&&self.accounting()?.ActiveProcesses==0{self.cleaned=true;return Ok(())}thread::sleep(Duration::from_millis(5));}Err("owned_cleanup_deadline")
    }
    pub fn finish(mut self,limit:Duration)->Result<Receipt>{
        if limit>Duration::from_secs(5){return Err("owned_observation_bound")}
        let end=Instant::now()+limit;while self.poll()?.is_none()&&Instant::now()<end{thread::sleep(Duration::from_millis(5));}
        let before=self.poll()?;let timed_out=before.is_none()&&self.resumed;self.stop()?;
        let receipt=Receipt{resumed:self.resumed,timed_out,before_cleanup:before,exit_code:self.poll()?,job_empty:self.accounting()?.ActiveProcesses==0,total_processes:self.accounting()?.TotalProcesses,stdout:self.stdout.take().ok_or("stdout_missing")?.finish()?,stderr:self.stderr.take().ok_or("stderr_missing")?.finish()?};
        let mut output=std::fs::OpenOptions::new().create_new(true).write(true).open(self.directory.join("summary.json")).map_err(|_|"private_receipt_create_failed")?;output.write_all(receipt.json().as_bytes()).and_then(|_|output.sync_all()).map_err(|_|"private_receipt_write_failed")?;Ok(receipt)
    }
}
impl Drop for Prepared{fn drop(&mut self){if !self.cleaned{let _=self.stop();}if let Some(drain)=self.stdout.take(){let _=drain.finish();}if let Some(drain)=self.stderr.take(){let _=drain.finish();}}}
fn duplicate(handle:HANDLE,rights:u32)->Result<Handle>{let mut copy=ptr::null_mut();yes(unsafe{DuplicateHandle(GetCurrentProcess(),handle,GetCurrentProcess(),&mut copy,rights,0,0)})?;Handle::new(copy)}
struct StartupAudit{process:Option<Handle>,job:Option<Handle>,metrics:Arc<DrainMetrics>}
/// Closed failure injection: still targets only the pinned authored sibling.
pub fn check_drain_failure(pin:[u8;32],second:bool)->Result<()>{
    let mut audit=StartupAudit{process:None,job:None,metrics:Arc::new(DrainMetrics::default())};let started=Instant::now();
    if !matches!(Prepared::new_inner(pin,Scenario::Marker,Some(second),Some(&mut audit)),Err("output_drain_start_failed")){return Err("drain_start_failure_not_observed")}
    let process=audit.process.as_ref().ok_or("injected_child_not_retained")?;let job=audit.job.as_ref().ok_or("injected_job_not_retained")?;
    if unsafe{WaitForSingleObject(process.0,2000)}!=WAIT_OBJECT_0{return Err("injected_child_survived")}
    let mut exit=0;yes(unsafe{GetExitCodeProcess(process.0,&mut exit)})?;if exit!=124{return Err("injected_child_exit_unexpected")}
    let mut data=JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();yes(unsafe{QueryInformationJobObject(job.0,JobObjectBasicAccountingInformation,(&mut data as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),std::mem::size_of_val(&data)as u32,ptr::null_mut())})?;
    let expected=second as u64;
    if data.ActiveProcesses!=0||audit.metrics.started.load(Ordering::SeqCst)!=expected||audit.metrics.eof.load(Ordering::SeqCst)!=expected||audit.metrics.active.load(Ordering::SeqCst)!=0||audit.metrics.bytes.load(Ordering::SeqCst)!=0||started.elapsed()>Duration::from_secs(5){return Err("injected_cleanup_not_verified")}Ok(())
}
pub fn retained_worker(pid:u32,pin:[u8;32])->Result<Handle>{let handle=Handle::new(unsafe{OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION|SYNCHRONIZE,0,pid)})?;let(_,image,_directories)=worker_image(pin)?;image_matches(handle.0,&image)?;Ok(handle)}
pub fn wait_retained(h:&Handle)->bool{unsafe{WaitForSingleObject(h.0,2000)==WAIT_OBJECT_0}}
use std::os::windows::ffi::OsStringExt;

#[derive(Clone,Copy)]pub(crate) enum Image{Worker,Guard,Launcher}
impl Image{fn name(self)->&'static str{match self{Self::Worker=>WORKER,Self::Guard=>"metallic-medium-guard.exe",Self::Launcher=>"metallic-medium-control.exe"}}}
pub(crate) fn process_stamp(process:HANDLE)->Result<crate::protocol::Stamp>{
 let(mut created,mut exited,mut kernel,mut user)=(FILETIME::default(),FILETIME::default(),FILETIME::default(),FILETIME::default());yes(unsafe{GetProcessTimes(process,&mut created,&mut exited,&mut kernel,&mut user)})?;
 let stamp=crate::protocol::Stamp{pid:unsafe{GetProcessId(process)},created:((created.dwHighDateTime as u64)<<32)|created.dwLowDateTime as u64};if !stamp.valid(){return Err("process_stamp_required")}Ok(stamp)
}
pub(crate) struct TokenFacts{sid:Vec<u8>,pub session:u32,pub elevated:bool,pub integrity:u32}
pub(crate) fn token_facts(process:HANDLE)->Result<TokenFacts>{
 let mut raw=ptr::null_mut();yes(unsafe{OpenProcessToken(process,TOKEN_QUERY,&mut raw)})?;let token=Handle::new(raw)?;
 fn info(token:HANDLE,kind:TOKEN_INFORMATION_CLASS)->Result<Vec<u64>>{let mut n=0;unsafe{GetTokenInformation(token,kind,ptr::null_mut(),0,&mut n)};if n==0||n>8192{return Err("token_size_refused")}let mut b=vec![0u64;(n as usize+7)/8];yes(unsafe{GetTokenInformation(token,kind,b.as_mut_ptr().cast(),n,&mut n)})?;Ok(b)}
 fn sid_bytes(b:&[u64],sid:PSID)->Result<Vec<u8>>{let base=b.as_ptr()as usize;let end=base+b.len()*8;let at=sid as usize;if at<base||at.checked_add(8).ok_or("sid_bounds")?>end{return Err("sid_bounds")}let count=unsafe{*((sid as *const u8).add(1))}as usize;let len=8+4*count;if count>15||at.checked_add(len).ok_or("sid_bounds")?>end||unsafe{IsValidSid(sid)}==0{return Err("sid_bounds")}Ok(unsafe{std::slice::from_raw_parts(sid as *const u8,len)}.to_vec())}
 let b=info(token.0,TokenUser)?;let user=unsafe{&*(b.as_ptr()as *const TOKEN_USER)};let sid=sid_bytes(&b,user.User.Sid)?;
 let b=info(token.0,TokenIntegrityLevel)?;let label=unsafe{&*(b.as_ptr()as *const TOKEN_MANDATORY_LABEL)};let il=sid_bytes(&b,label.Label.Sid)?;let integrity=u32::from_le_bytes(il[il.len()-4..].try_into().map_err(|_|"integrity_required")?);
 let mut session=0u32;let mut size=0;yes(unsafe{GetTokenInformation(token.0,TokenSessionId,(&mut session as *mut u32).cast(),4,&mut size)})?;
 let mut elevation=TOKEN_ELEVATION::default();yes(unsafe{GetTokenInformation(token.0,TokenElevation,(&mut elevation as *mut TOKEN_ELEVATION).cast(),std::mem::size_of_val(&elevation)as u32,&mut size)})?;
 Ok(TokenFacts{sid,session,elevated:elevation.TokenIsElevated!=0,integrity})
}
pub(crate) struct Peer{pub handle:Handle,pub stamp:crate::protocol::Stamp}
pub(crate) fn verify_peer(handle:Handle,stamp:crate::protocol::Stamp,image:&File,owner:&TokenFacts,elevated:bool)->Result<Peer>{
 image_matches(handle.0,image)?;let actual=process_stamp(handle.0)?;let token=token_facts(handle.0)?;
 crate::protocol::admit_peer(crate::protocol::PeerFacts{expected_pid:stamp.pid,actual_pid:actual.pid,expected_created:stamp.created,actual_created:actual.created,same_image:true,same_hash:true,same_sid:token.sid==owner.sid,same_session:token.session==owner.session,expected_elevated:elevated,actual_elevated:token.elevated,expected_integrity:if elevated{12288}else{8192},actual_integrity:token.integrity})?;
 Ok(Peer{handle,stamp:actual})
}
pub(crate) fn open_peer(stamp:crate::protocol::Stamp,image:&File,owner:&TokenFacts,elevated:bool)->Result<Peer>{let handle=Handle::new(unsafe{OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION|SYNCHRONIZE,0,stamp.pid)})?;verify_peer(handle,stamp,image,owner,elevated)}
pub(crate) fn exited(peer:&Peer)->Result<bool>{match unsafe{WaitForSingleObject(peer.handle.0,0)}{WAIT_OBJECT_0=>Ok(true),WAIT_TIMEOUT=>Ok(false),_=>Err("retained_peer_wait_failed")}}
pub(crate) fn verify_guard_load_flags(image:&File)->Result<()>{use std::io::{Seek,SeekFrom};let mut copy=image.try_clone().map_err(|_|"image_clone_failed")?;copy.seek(SeekFrom::Start(0)).map_err(|_|"image_seek_failed")?;let mut bytes=Vec::new();copy.take(16*1024*1024+1).read_to_end(&mut bytes).map_err(|_|"image_read_failed")?;crate::pe::guard_system32_imports(&bytes)}

/// Nonprivileged admission check over only a freshly created authored worker.
pub fn check_worker_identity(pin:[u8;32])->Result<()>{
 let target=Prepared::new(pin,Scenario::Marker)?;let stamp=target.stamp()?;let(_,image,_locks)=fixed_image(Image::Worker,pin)?;let owner=token_facts(unsafe{GetCurrentProcess()})?;
 let retain=||duplicate(target.process.0,PROCESS_QUERY_LIMITED_INFORMATION|SYNCHRONIZE);
 verify_peer(retain()?,stamp,&image,&owner,false)?;
 let mut wrong=stamp;wrong.pid=wrong.pid.checked_add(1).ok_or("test_pid_bound")?;if verify_peer(retain()?,wrong,&image,&owner,false).is_ok(){return Err("wrong_pid_accepted")}
 wrong=stamp;wrong.created=wrong.created.checked_add(1).ok_or("test_time_bound")?;if verify_peer(retain()?,wrong,&image,&owner,false).is_ok(){return Err("wrong_creation_time_accepted")}
 let mut wrong_owner=token_facts(unsafe{GetCurrentProcess()})?;wrong_owner.sid[0]^=1;if verify_peer(retain()?,stamp,&image,&wrong_owner,false).is_ok(){return Err("wrong_sid_accepted")}
 let mut wrong_owner=token_facts(unsafe{GetCurrentProcess()})?;wrong_owner.session^=1;if verify_peer(retain()?,stamp,&image,&wrong_owner,false).is_ok(){return Err("wrong_session_accepted")}
 if verify_peer(retain()?,stamp,&image,&owner,true).is_ok(){return Err("wrong_elevation_accepted")}
 let own=lock_image(&std::env::current_exe().map_err(|_|"checker_image_unavailable")?)?;if verify_peer(retain()?,stamp,&own,&owner,false).is_ok(){return Err("wrong_image_accepted")}
 let receipt=target.finish(Duration::ZERO)?;if receipt.resumed||!receipt.job_empty||receipt.stdout.bytes!=0||!receipt.stdout.eof||!receipt.stderr.eof{return Err("admission_test_cleanup_failed")}Ok(())
}
