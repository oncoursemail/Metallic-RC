//! Non-admin authored-worker process plumbing. Not an original-client executor.
pub const ENVIRONMENT:[&str;20]=["SystemRoot","SystemDrive","WINDIR","PATH","COMSPEC","TEMP","TMP","LOCALAPPDATA","APPDATA","USERPROFILE","HOMEDRIVE","HOMEPATH","ProgramData","ProgramFiles","ProgramFiles(x86)","PUBLIC","USERNAME","USERDOMAIN","NUMBER_OF_PROCESSORS","PROCESSOR_ARCHITECTURE"];
pub const WORKER:&str="metallic-medium-worker.exe";
pub type Result<T>=std::result::Result<T,&'static str>;

#[derive(Clone,Copy,Debug)]
pub enum Scenario { Marker, Environment, Flood, Wait, Grandchild, Network(metallic_external_guard_fixture::Probe,u16) }
impl Scenario {
    pub fn argument(self)->String{match self{Self::Marker=>"--marker".into(),Self::Environment=>"--environment".into(),Self::Flood=>"--flood".into(),Self::Wait=>"--wait".into(),Self::Grandchild=>"--grandchild".into(),Self::Network(probe,port)=>format!("--network-probe {} {port}",probe.name())}}
}
pub fn digest(text:&str)->Result<[u8;32]>{
    if text.len()!=64||!text.bytes().all(|c|c.is_ascii_hexdigit()){return Err("reviewed_worker_sha256_required")}
    let mut bytes=[0;32];for (i,b) in bytes.iter_mut().enumerate(){*b=u8::from_str_radix(&text[i*2..i*2+2],16).map_err(|_|"reviewed_worker_sha256_required")?;}Ok(bytes)
}
pub fn environment_block(rows:impl IntoIterator<Item=(String,String)>)->Result<(Vec<u16>,usize,bool)>{
    let mut selected=std::collections::BTreeMap::new();
    for (name,value) in rows {
        if let Some(expected)=ENVIRONMENT.iter().find(|key|key.eq_ignore_ascii_case(&name)){
            if value.contains('\0')||selected.insert(expected.to_ascii_uppercase(),(expected.to_string(),value)).is_some(){return Err("ambiguous_environment")}
        }
    }
    let count=selected.len();let drive=selected.contains_key("SYSTEMDRIVE");let mut block=Vec::new();
    for (_, (name,value)) in selected{block.extend(format!("{name}={value}").encode_utf16());block.push(0);}
    if block.is_empty(){block.push(0)}block.push(0);Ok((block,count,drive))
}

pub fn verify_wrong_pin_refusal<T>(outcome:Result<T>)->Result<()>{
    match outcome{
        Err("worker_pin_mismatch")=>Ok(()),
        Err("non_admin_fixture_required")=>Err("non_admin_fixture_required"),
        _=>Err("wrong_pin_refusal_not_verified"),
    }
}

pub mod protocol;
pub mod pe;
pub mod network_protocol;
pub mod network_probe;
#[cfg(windows)] mod process;
#[cfg(windows)] mod ipc;
#[cfg(windows)] mod wfp;
#[cfg(windows)] pub mod control;
// Closed authored-worker roles only; no original-client target or commands.
#[cfg(windows)] mod network_control;
#[cfg(windows)] pub use network_control::check_network_workers;
#[cfg(windows)] pub fn run_network_control(launcher:[u8;32],worker:[u8;32],guard:[u8;32])->Result<()>{network_control::launcher(launcher,worker,guard)}
#[cfg(windows)] pub fn run_network_guard(nonce:[u8;16],parent:protocol::Stamp,launcher:[u8;32],worker:[u8;32],guard:[u8;32])->Result<()>{network_control::guard(nonce,parent,launcher,worker,guard)}
#[cfg(windows)] pub use process::{Prepared,Receipt,retained_worker,wait_retained,check_drain_failure,check_worker_identity};

#[cfg(test)]mod tests{
    use super::*;
    #[test]fn wrong_pin_outcomes_preserve_only_exact_pre_admission_refusal(){
        assert_eq!(verify_wrong_pin_refusal::<()>(Err("worker_pin_mismatch")),Ok(()));
        assert_eq!(verify_wrong_pin_refusal::<()>(Err("non_admin_fixture_required")),Err("non_admin_fixture_required"));
        assert_eq!(verify_wrong_pin_refusal(Ok(())),Err("wrong_pin_refusal_not_verified"));
        assert_eq!(verify_wrong_pin_refusal::<()>(Err("other_failure")),Err("wrong_pin_refusal_not_verified"));
    }
    #[test]fn pin_requires_exact_hex(){assert!(digest("").is_err());assert!(digest(&"z".repeat(64)).is_err());assert_eq!(digest(&"a1".repeat(32)).unwrap(),[0xa1;32]);}
    #[test]fn unicode_environment_is_closed_and_double_terminated(){
        let (b,n,drive)=environment_block([(String::from("Path"),String::from("owned")),("SYSTEMDRIVE".into(),"C:".into()),("SECRET_TOKEN".into(),"omit".into()),("TEMP".into(),"synthetic-猫".into())]).unwrap();
        assert_eq!(n,3);assert!(drive);assert_eq!(&b[b.len()-2..],&[0,0]);let text=String::from_utf16(&b).unwrap();assert!(text.contains("TEMP=synthetic-猫"));assert!(!text.contains("SECRET"));assert!(!text.contains("omit"));
    }
    #[test]fn duplicate_or_nul_allowed_environment_refuses(){assert!(environment_block([("PATH".into(),"a".into()),("Path".into(),"b".into())]).is_err());assert!(environment_block([("TEMP".into(),"a\0b".into())]).is_err());}
    #[test]fn scenarios_are_closed_fixed_arguments(){assert_eq!([Scenario::Marker,Scenario::Environment,Scenario::Flood,Scenario::Wait,Scenario::Grandchild].map(Scenario::argument),["--marker","--environment","--flood","--wait","--grandchild"]);}
}
