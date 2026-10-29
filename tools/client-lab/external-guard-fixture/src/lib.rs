//! Fixture policy and lifecycle. Unit tests use only the fake API below.
pub type Result<T> = std::result::Result<T, &'static str>;
pub mod kernel;
use std::{sync::{Arc,atomic::{AtomicBool,Ordering}},time::{Duration,Instant}};
// Explicit INDEXED-only profile; every additional or missing bit is refused.
pub const REQUESTED_FILTER_FLAGS:u32=0x40;

/// Failure-only, bounded numeric metadata. No observed path/blob is accepted.
pub fn failure_report(error:&'static str,observed_flags:Option<u32>,probe:Option<ProbeFailure>)->String {
    let mut numeric=match observed_flags {
        Some(flags)=>format!(",\"requestedFlags\":{REQUESTED_FILTER_FLAGS},\"observedFlags\":{flags}"),
        None=>String::new(),
    };
    if let Some(probe)=probe {
        let actual=probe.actual.map(|code|code.to_string()).unwrap_or_else(||"null".to_owned());
        numeric.push_str(&format!(",\"probePhase\":\"{}\",\"probeProtocol\":\"{}\",\"expectedChildExit\":{},\"actualChildExit\":{actual}",probe.phase.name(),probe.protocol.name(),probe.phase.expected()));
        if let Some(error)=reported_socket_error(probe.actual){numeric.push_str(&format!(",\"childReportedSocketError\":{error}"));}
        if let Some(udp)=probe.udp{
            numeric.push_str(&format!(",\"kernelDiagnostic\":\"{}\",\"kernelOutboundDrops\":{},\"kernelInboundDrops\":{}",udp.stats.status.name(),udp.stats.outbound,udp.stats.inbound));
            if let Some(api)=udp.stats.api{numeric.push_str(&format!(",\"kernelApiStage\":\"{}\",\"kernelApiCode\":{}",api.stage.name(),api.code));}
            if let Some(record)=udp.record{numeric.push_str(&format!(",\"udpOperation\":\"{}\",\"udpSendBytes\":{}",record.operation.name(),record.sent));}
        }
    }
    format!("{{\"ok\":false,\"error\":\"{error}\",\"cleanupVerified\":false,\"originalClientTested\":false,\"executionReady\":false{numeric}}}")
}

#[derive(Clone)]
pub struct Budget { start:Instant, expired:Arc<AtomicBool> }
impl Default for Budget { fn default()->Self{Self{start:Instant::now(),expired:Arc::new(AtomicBool::new(false))}} }
impl Budget {
    pub fn check(&self)->Result<()>{admit_elapsed(self.start.elapsed(),&self.expired)}
    pub fn expire(&self){self.expired.store(true,Ordering::SeqCst);}
}
pub fn admit_elapsed(elapsed:Duration,expired:&AtomicBool)->Result<()>{
    if elapsed>=Duration::from_secs(90){expired.store(true,Ordering::SeqCst);}
    if expired.load(Ordering::SeqCst){Err("fixture_deadline")}else{Ok(())}
}
pub fn timeouts_ready(read_ready:bool,write_ready:bool)->Result<()>{if read_ready&&write_ready{Ok(())}else{Err("listener_timeouts_unavailable")}}
/// Only an explicit Winsock access-denied error is a blocked observation.
/// Missing replies, timeouts, refusal and protocol failures are inconclusive.
pub fn classify_probe(exchange:std::result::Result<bool,Option<i32>>)->i32{
    match exchange{Ok(true)=>0,Err(Some(10013))=>20,Err(Some(error)) if (1..=65535).contains(&error)=>0x10000|error,_=>21}
}
/// This is the authored child's diagnostic namespace, not OS attestation.
pub fn reported_socket_error(actual:Option<i32>)->Option<u16>{
    let code=actual?;
    if !(0x10001..=0x1ffff).contains(&code){return None}
    let error=code&0xffff;
    if classify_probe(Err(Some(error)))!=code{return None}
    Some(error as u16)
}
pub fn accepted_probe_exit(actual:Option<i32>,expected:i32)->bool{
    matches!(expected,0|20)&&actual==Some(expected)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer { Connect4, Connect6, Receive4, Receive6 }
pub const LAYERS: [Layer; 4] = [Layer::Connect4, Layer::Connect6, Layer::Receive4, Layer::Receive6];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rule {
    pub id: u64,
    pub key: [u8; 16],
    pub sublayer: [u8; 16],
    pub layer: Layer,
    pub app_id: Vec<u8>,
    pub block: bool,
    pub flags: u32,
    pub conditions: u32,
    pub app_equal: bool,
}

/// Report only a static first-mismatch category, never observed path/blob data.
/// The final equality check preserves exact acceptance if Rule gains a field.
pub fn check_readback(expected:&Rule, actual:Option<&Rule>)->Result<()> {
    let actual=actual.ok_or("readback_missing")?;
    if actual.id!=expected.id{return Err("readback_id_mismatch")}
    if actual.key!=expected.key{return Err("readback_key_mismatch")}
    if actual.sublayer!=expected.sublayer{return Err("readback_sublayer_mismatch")}
    if actual.layer!=expected.layer{return Err("readback_layer_mismatch")}
    if actual.app_id!=expected.app_id{return Err("readback_app_id_mismatch")}
    if actual.block!=expected.block{return Err("readback_action_mismatch")}
    if actual.flags!=expected.flags{return Err("readback_flags_mismatch")}
    if actual.conditions!=expected.conditions{return Err("readback_condition_count_mismatch")}
    if actual.app_equal!=expected.app_equal{return Err("readback_match_type_mismatch")}
    if actual!=expected{return Err("readback_other_mismatch")}
    Ok(())
}

pub trait Api {
    fn open_dynamic(&mut self) -> Result<()>;
    fn begin(&mut self) -> Result<()>;
    fn add_sublayer(&mut self) -> Result<()>;
    fn add(&mut self, layer: Layer) -> Result<Rule>;
    fn commit(&mut self) -> Result<()>;
    fn read(&mut self, id: u64) -> Result<Option<Rule>>;
    fn abort(&mut self) -> Result<()>;
    fn delete(&mut self, id: u64) -> Result<()>;
    fn delete_sublayer(&mut self) -> Result<()>;
    fn close(&mut self) -> Result<()>;
}

/// Any admission error closes the dynamic session before probes are allowed.
/// Cleanup errors remain failures; they never turn admission into success.
pub fn install(api: &mut impl Api) -> Result<Vec<Rule>> {
    api.open_dynamic()?;
    let mut committed = false;
    let result = (|| {
        api.begin()?;
        api.add_sublayer()?;
        let mut rules = Vec::new();
        for layer in LAYERS {
            let rule=api.add(layer)?;
            if rule.id==0 || rule.layer!=layer || !rule.block || rule.flags!=REQUESTED_FILTER_FLAGS || rule.conditions!=1 || !rule.app_equal || rule.app_id.is_empty() || rule.app_id.len()>32768 { return Err("rule_policy_mismatch"); }
            if rules.iter().any(|r:&Rule|r.id==rule.id||r.key==rule.key||r.app_id!=rule.app_id||r.sublayer!=rule.sublayer){return Err("rule_identity_mismatch")}
            rules.push(rule);
        }
        api.commit()?;
        committed = true;
        for rule in &rules {
            check_readback(rule,api.read(rule.id)?.as_ref())?;
        }
        Ok(rules)
    })();
    if result.is_err() {
        if !committed { let _ = api.abort(); }
        if api.close().is_err() { return Err("admission_cleanup_failed"); }
    }
    result
}

/// Call only after the owned job proves zero active probe children.
pub fn cleanup(api: &mut impl Api, rules: &[Rule], children_empty: bool) -> Result<()> {
    if !children_empty { return Err("probe_children_pending_guard_retained"); }
    let mut failed = false;
    for rule in rules { if api.delete(rule.id).is_err() { failed = true; } }
    for rule in rules {
        if !matches!(api.read(rule.id), Ok(None)) { failed = true; }
    }
    if api.delete_sublayer().is_err() { failed = true; }
    if api.close().is_err() { failed = true; }
    if failed { Err("filter_cleanup_unverified") } else { Ok(()) }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Probe { Tcp4, Tcp6, Udp4, Udp6 }
pub const PROBES: [Probe; 4] = [Probe::Tcp4, Probe::Tcp6, Probe::Udp4, Probe::Udp6];
impl Probe {
    pub fn name(self) -> &'static str { match self { Self::Tcp4 => "tcp4", Self::Tcp6 => "tcp6", Self::Udp4 => "udp4", Self::Udp6 => "udp6" } }
    pub fn host(self) -> &'static str { match self { Self::Tcp4 | Self::Udp4 => "127.0.0.1", _ => "::1" } }
    pub fn tcp(self) -> bool { matches!(self, Self::Tcp4 | Self::Tcp6) }
}

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum ProbePhase { Before, Blocked, After }
impl ProbePhase {
    pub fn name(self)->&'static str{match self{Self::Before=>"before",Self::Blocked=>"blocked",Self::After=>"after"}}
    pub fn expected(self)->i32{match self{Self::Blocked=>20,_=>0}}
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct ProbeFailure { pub phase:ProbePhase, pub protocol:Probe, pub actual:Option<i32>, pub udp:Option<kernel::Observation> }
fn udp_timeout_metadata(info:&ProbeFailure)->Option<kernel::Observation>{
    use kernel::{Operation,Record};
    let family=match info.protocol{Probe::Udp4=>4,Probe::Udp6=>6,_=>return None};
    if info.phase!=ProbePhase::Blocked||reported_socket_error(info.actual)!=Some(10060){return None}
    let observation=info.udp?;let record=observation.record?;
    if record.family!=family||record.operation!=Operation::Receive||record.sent!=4||
        Record::decode(&record.encode(),info.protocol,record.destination_port)!=Some(record)||
        observation.start==0||observation.end<observation.start||observation.end-observation.start>100_000_000{return None}
    Some(observation)
}
/// The native callback must correlate this observation with current verified rules.
pub fn confirmed_blocked_udp(info:&ProbeFailure)->bool{
    matches!(udp_timeout_metadata(info),Some(observation)if observation.stats.status==kernel::Status::Matched&&observation.stats.outbound>0&&observation.stats.api.is_none())
}
pub fn resolve_probe(info:&mut ProbeFailure,diagnose:impl FnOnce(kernel::Observation)->kernel::Stats)->Result<()>{
    if accepted_probe_exit(info.actual,info.phase.expected()){return Ok(())}
    if let Some(observation)=udp_timeout_metadata(info){
        info.udp.as_mut().unwrap().stats=diagnose(observation);
        if confirmed_blocked_udp(info){return Ok(())}
    }
    Err("probe_outcome_mismatch")
}
pub fn run_probe_phase(phase:ProbePhase,mut attempt:impl FnMut(Probe,ProbePhase)->Result<()>)->Result<()>{
    for protocol in PROBES{attempt(protocol,phase)?;}Ok(())
}
pub fn finish_probe(outcome:Result<()>,context:&mut Option<ProbeFailure>)->Result<()>{
    if outcome.is_ok(){*context=None;}outcome
}

#[derive(Debug, PartialEq, Eq)]
pub enum Role { Plan, Fixture, Probe(Probe, u16) }
pub fn role(args: &[String]) -> Result<Role> {
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        [] | ["--plan"] => Ok(Role::Plan),
        ["--fixture"] => Ok(Role::Fixture),
        ["--probe", name, port] => {
            let probe = PROBES.into_iter().find(|p| p.name() == *name).ok_or("probe_role_refused")?;
            let port: u16 = port.parse().map_err(|_| "probe_port_refused")?;
            if port == 0 { return Err("probe_port_refused"); }
            Ok(Role::Probe(probe, port))
        }
        _ => Err("arguments_refused"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Default)]
    struct Fake { events: Vec<String>, rules: Vec<Rule>, fail: Option<&'static str>, mismatch: bool, wrong_field:u8, admission_flags:Option<u32> }
    impl Fake {
        fn event(&mut self, name: &str) -> Result<()> { self.events.push(name.into()); if self.fail == Some(name) { Err("injected") } else { Ok(()) } }
    }
    impl Api for Fake {
        fn open_dynamic(&mut self) -> Result<()> { self.event("open_dynamic") }
        fn begin(&mut self) -> Result<()> { self.event("begin") }
        fn add_sublayer(&mut self) -> Result<()> { self.event("sublayer") }
        fn add(&mut self, layer: Layer) -> Result<Rule> {
            self.event(&format!("add_{}", self.rules.len()))?;
            let r = Rule { id: self.rules.len() as u64+1, key: [self.rules.len() as u8;16], sublayer:[9;16], layer, app_id: vec![4,2], block:true, flags:self.admission_flags.unwrap_or(REQUESTED_FILTER_FLAGS), conditions:1, app_equal:true };
            self.rules.push(r.clone()); Ok(r)
        }
        fn commit(&mut self) -> Result<()> { self.event("commit") }
        fn read(&mut self, id: u64) -> Result<Option<Rule>> {
            self.event("read")?;
            let mut r = self.rules.iter().find(|r| r.id == id).cloned();
            if self.mismatch { if let Some(r) = &mut r { r.block=false; } }
            if let Some(r)=&mut r {match self.wrong_field{1=>r.id+=1,2=>r.key[0]^=1,3=>r.sublayer[0]^=1,4=>r.layer=Layer::Receive6,5=>r.app_id.push(0),6=>r.flags=1,7=>r.conditions=2,8=>r.app_equal=false,_=>{}}}
            Ok(r)
        }
        fn abort(&mut self) -> Result<()> { self.event("abort") }
        fn delete(&mut self, id: u64) -> Result<()> { self.event("delete")?; self.rules.retain(|r| r.id != id); Ok(()) }
        fn delete_sublayer(&mut self) -> Result<()> { self.event("delete_sublayer") }
        fn close(&mut self) -> Result<()> { self.event("close")?; self.rules.clear(); Ok(()) }
    }
    #[test] fn install_exact_four_and_read_after_commit() {
        let mut f=Fake::default(); let rules=install(&mut f).unwrap();
        assert_eq!(rules.iter().map(|r| r.layer).collect::<Vec<_>>(), LAYERS);
        assert_eq!(&f.events[8..], ["read","read","read","read"]);
    }
    #[test] fn each_partial_admission_failure_closes_without_success() {
        for step in ["begin","sublayer","add_0","add_1","add_2","add_3","commit","read"] {
            let mut f=Fake { fail:Some(step), ..Default::default() }; assert!(install(&mut f).is_err()); assert_eq!(f.events.last().unwrap(),"close"); assert!(f.rules.is_empty());
        }
    }
    #[test] fn readback_mismatch_fails() { let mut f=Fake{mismatch:true,..Default::default()}; assert_eq!(install(&mut f),Err("readback_action_mismatch")); assert!(f.rules.is_empty()); }
    #[test] fn each_readback_field_is_checked(){
        for (wrong_field,category) in [(1,"readback_id_mismatch"),(2,"readback_key_mismatch"),(3,"readback_sublayer_mismatch"),(4,"readback_layer_mismatch"),(5,"readback_app_id_mismatch"),(6,"readback_flags_mismatch"),(7,"readback_condition_count_mismatch"),(8,"readback_match_type_mismatch")]{
            let mut f=Fake{wrong_field,..Default::default()};assert_eq!(install(&mut f),Err(category));assert!(f.rules.is_empty());assert_eq!(f.events.last().unwrap(),"close");
        }
    }
    #[test] fn absent_and_equal_readbacks_keep_exact_acceptance(){
        let mut f=Fake::default();let r=install(&mut f).unwrap().remove(0);
        assert_eq!(check_readback(&r,None),Err("readback_missing"));assert_eq!(check_readback(&r,Some(&r)),Ok(()));
    }
    #[test] fn first_mismatch_stays_static_and_contains_no_observed_values(){
        let mut f=Fake::default();let r=install(&mut f).unwrap().remove(0);let mut actual=r.clone();
        actual.app_id=b"synthetic-private-path-marker".to_vec();actual.flags=u32::MAX;
        assert_eq!(check_readback(&r,Some(&actual)),Err("readback_app_id_mismatch"));
        actual.app_id=r.app_id.clone();assert_eq!(check_readback(&r,Some(&actual)),Err("readback_flags_mismatch"));
    }
    #[test] fn children_pending_does_not_remove_guard() { let mut f=Fake::default();let rules=install(&mut f).unwrap();let n=f.events.len();assert!(cleanup(&mut f,&rules,false).is_err());assert_eq!(n,f.events.len()); }
    #[test] fn cleanup_verifies_all_absent() { let mut f=Fake::default();let rules=install(&mut f).unwrap();cleanup(&mut f,&rules,true).unwrap();assert!(f.rules.is_empty()); }
    #[test] fn deletion_failure_is_not_success() { let mut f=Fake::default();let rules=install(&mut f).unwrap();f.fail=Some("delete");assert!(cleanup(&mut f,&rules,true).is_err());assert_eq!(f.events.last().unwrap(),"close"); }
    #[test] fn cleanup_read_error_is_not_absence() { let mut f=Fake::default();let rules=install(&mut f).unwrap();f.fail=Some("read");assert!(cleanup(&mut f,&rules,true).is_err()); }
    #[test] fn cleanup_close_failure_is_not_success() { let mut f=Fake::default();let rules=install(&mut f).unwrap();f.fail=Some("close");assert!(cleanup(&mut f,&rules,true).is_err()); }
    #[test] fn role_cannot_select_target_or_command() { for a in [vec!["--fixture","RecRoom.exe"],vec!["--probe","http://example.com","80"],vec!["--target","x"],vec!["--probe","tcp4","0"]] {assert!(role(&a.into_iter().map(str::to_owned).collect::<Vec<_>>()).is_err());} }
    #[test] fn plan_and_loopback_roles_are_closed() {assert_eq!(role(&[]),Ok(Role::Plan));for p in PROBES{assert_eq!(role(&["--probe".into(),p.name().into(),"1234".into()]),Ok(Role::Probe(p,1234)));assert!(["127.0.0.1","::1"].contains(&p.host()));} }
    #[test] fn deadline_latches_and_rejects_late_phase_completion(){let e=AtomicBool::new(false);assert!(admit_elapsed(Duration::from_secs(89),&e).is_ok());assert!(admit_elapsed(Duration::from_secs(90),&e).is_err());assert!(admit_elapsed(Duration::from_secs(1),&e).is_err());}
    #[test] fn watchdog_latch_rejects_even_before_clock_deadline(){let b=Budget::default();assert!(b.check().is_ok());b.expire();assert!(b.check().is_err());}
    #[test] fn blocking_io_requires_both_timeouts(){for a in [false,true]{for b in [false,true]{assert_eq!(timeouts_ready(a,b).is_ok(),a&&b);}}}
    #[test] fn only_explicit_winsock_access_denied_counts_as_block(){
        assert_eq!(classify_probe(Err(Some(10013))),20);
        for error in [10060,10061,10035,5]{assert_eq!(classify_probe(Err(Some(error))),0x10000|error);}
        for error in [Some(0),Some(-1),Some(65536),None]{assert_eq!(classify_probe(Err(error)),21);}
        assert_eq!(classify_probe(Ok(false)),21);assert_eq!(classify_probe(Ok(true)),0);
    }
    #[test] fn unexpected_or_crash_child_exits_never_pass(){
        assert!(accepted_probe_exit(Some(0),0));assert!(accepted_probe_exit(Some(20),20));
        for code in [None,Some(1),Some(21),Some(124),Some(0xc0000005u32 as i32)]{assert!(!accepted_probe_exit(code,20));assert!(!accepted_probe_exit(code,0));}
        assert!(!accepted_probe_exit(Some(21),21));assert!(!accepted_probe_exit(Some(0),20));assert!(!accepted_probe_exit(Some(20),0));
    }
    #[test] fn failure_without_observation_preserves_original_schema(){
        assert_eq!(REQUESTED_FILTER_FLAGS,0x40);
        assert_eq!(failure_report("readback_missing",None,None),"{\"ok\":false,\"error\":\"readback_missing\",\"cleanupVerified\":false,\"originalClientTested\":false,\"executionReady\":false}");
    }
    #[test] fn numeric_failure_metadata_is_bounded_unsigned_and_never_success(){
        for (flags,decimal) in [(0,"0"),(64,"64"),(u32::MAX,"4294967295")] {
            let report=failure_report("readback_flags_mismatch",Some(flags),None);
            assert_eq!(report,format!("{{\"ok\":false,\"error\":\"readback_flags_mismatch\",\"cleanupVerified\":false,\"originalClientTested\":false,\"executionReady\":false,\"requestedFlags\":64,\"observedFlags\":{decimal}}}"));
            assert!(report.len()<2048);
        }
    }
    #[test] fn indexed_only_profile_is_accepted_in_admission_and_readback(){
        let mut f=Fake{admission_flags:Some(0x40),..Default::default()};
        let rules=install(&mut f).unwrap();assert_eq!(rules.len(),4);
        for r in &rules{assert_eq!(r.flags,0x40);assert_eq!(check_readback(r,Some(r)),Ok(()));}
    }
    #[test] fn all_missing_extra_and_unknown_flag_bits_are_rejected(){
        let mut valid=Fake::default();let expected=install(&mut valid).unwrap().remove(0);
        let mut invalid=vec![0,0x20,0x41,0x60,u32::MAX];
        for bit in 0..32{let value=1u32<<bit;if value!=0x40{invalid.push(value);invalid.push(0x40|value);}}
        for flags in invalid{
            let mut f=Fake{admission_flags:Some(flags),..Default::default()};
            assert_eq!(install(&mut f),Err("rule_policy_mismatch"));
            assert!(f.rules.is_empty());assert!(!f.events.iter().any(|e|e=="commit"));
            assert_eq!(f.events.last().unwrap(),"close");
            let mut actual=expected.clone();actual.flags=flags;
            assert_eq!(check_readback(&expected,Some(&actual)),Err("readback_flags_mismatch"));
        }
    }
    #[test] fn all_bounded_errors_use_canonical_tags_and_never_pass(){
        for error in 1..=65535{
            let code=classify_probe(Err(Some(error)));let tag=0x10000|error;
            assert!(!accepted_probe_exit(Some(tag),0));assert!(!accepted_probe_exit(Some(tag),20));
            if error==10013{assert_eq!(code,20);assert_eq!(reported_socket_error(Some(tag)),None);}
            else{assert_eq!(code,tag);assert_eq!(reported_socket_error(Some(code)),Some(error as u16));}
        }
        for code in [None,Some(0x10000),Some(0x20001),Some(-1),Some(0xc0000005u32 as i32),Some(21),Some(1),Some(20)]{assert_eq!(reported_socket_error(code),None);}
    }
    #[test] fn probe_failure_context_clears_only_after_complete_success(){
        let info=ProbeFailure{phase:ProbePhase::Before,protocol:Probe::Tcp6,actual:None,udp:None};
        let mut context=Some(info);assert_eq!(finish_probe(Err("probe_deadline"),&mut context),Err("probe_deadline"));assert_eq!(context,Some(info));
        context.as_mut().unwrap().actual=Some(0);finish_probe(Ok(()),&mut context).unwrap();assert_eq!(context,None);
        let report=failure_report("readback_flags_mismatch",Some(64),context);assert!(!report.contains("probePhase"));
    }
    #[test] fn kernel_diagnostics_never_promote_failure_or_expose_tuple(){
        use kernel::{Observation,Operation,Record,Stats,Status};
        for status in [Status::Unavailable,Status::Disabled,Status::Inconclusive,Status::Matched,Status::Capped,Status::Deadline]{
            let udp=Observation{record:Some(Record{family:4,operation:Operation::Receive,source_port:42123,destination_port:52123,sent:4}),start:100,end:200,stats:Stats{status,outbound:1,inbound:2,api:None}};
            let info=ProbeFailure{phase:ProbePhase::Blocked,protocol:Probe::Udp4,actual:Some(75596),udp:Some(udp)};
            let report=failure_report("probe_outcome_mismatch",Some(64),Some(info));
            assert!(report.starts_with("{\"ok\":false,"));assert!(report.contains("\"cleanupVerified\":false"));
            assert!(report.contains("\"kernelOutboundDrops\":1"));assert!(report.contains("\"kernelInboundDrops\":2"));
            assert!(report.contains("\"udpSendBytes\":4"));assert!(!report.contains("42123"));assert!(!report.contains("52123"));assert!(report.len()<2048);
            assert!(!accepted_probe_exit(info.actual,20));let mut context=Some(info);assert!(finish_probe(Err("probe_outcome_mismatch"),&mut context).is_err());assert_eq!(context,Some(info));
        }
    }
    #[test] fn kernel_api_failure_fields_are_closed_unsigned_json_and_optional(){
        use kernel::{ApiFailure,ApiStage,Observation,Stats,Status};
        let mut probe=ProbeFailure{phase:ProbePhase::Blocked,protocol:Probe::Udp4,actual:Some(75596),udp:Some(Observation::default())};
        let report=failure_report("probe_outcome_mismatch",None,Some(probe));assert!(!report.contains("kernelApiStage"));assert!(!report.contains("kernelApiCode"));
        for(stage,name)in[(ApiStage::CollectionGet,"collection_get"),(ApiStage::LayerGet,"layer_get"),(ApiStage::EnumCreate,"enum_create"),(ApiStage::EnumPage,"enum_page"),(ApiStage::EnumDestroy,"enum_destroy")]{for code in [5,50,0x8032000d,u32::MAX]{
            probe.udp.as_mut().unwrap().stats=Stats{api:Some(ApiFailure{stage,code}),..Stats::empty(Status::Unavailable)};
            let report=failure_report("probe_outcome_mismatch",None,Some(probe));assert!(report.contains(&format!("\"kernelApiStage\":\"{name}\",\"kernelApiCode\":{code}")));
            assert!(!report.contains(&format!("\"kernelApiCode\":\"{code}\"")));assert!(report.starts_with("{\"ok\":false,"));assert!(report.len()<2048);assert!(!accepted_probe_exit(probe.actual,20));
        }}
    }
    #[test] fn probe_failure_report_uses_closed_phase_protocol_and_retained_exit(){
        for phase in [ProbePhase::Before,ProbePhase::Blocked,ProbePhase::After]{for protocol in PROBES{
            let code=classify_probe(Err(Some(10060)));let info=ProbeFailure{phase,protocol,actual:Some(code),udp:None};
            let report=failure_report("probe_outcome_mismatch",Some(64),Some(info));
            assert!(report.contains(&format!("\"probePhase\":\"{}\"",phase.name())));assert!(report.contains(&format!("\"probeProtocol\":\"{}\"",protocol.name())));
            assert!(report.contains(&format!("\"expectedChildExit\":{}",phase.expected())));assert!(report.contains("\"actualChildExit\":75596"));assert!(report.contains("\"childReportedSocketError\":10060"));assert!(report.starts_with("{\"ok\":false,"));assert!(report.len()<2048);
        }}
        for actual in [None,Some(21),Some(0xc0000005u32 as i32),Some(0x10000|10013)]{
            let report=failure_report("probe_outcome_mismatch",None,Some(ProbeFailure{phase:ProbePhase::Blocked,protocol:Probe::Udp4,actual,udp:None}));
            assert!(!report.contains("childReportedSocketError"));
            if actual.is_none(){assert!(report.contains("\"actualChildExit\":null"));}
        }
    }

    fn timeout_info(protocol:Probe)->ProbeFailure{
        use kernel::{Observation,Operation,Record,Stats,Status};
        ProbeFailure{phase:ProbePhase::Blocked,protocol,actual:Some(75596),udp:Some(Observation{
            record:Some(Record{family:if protocol==Probe::Udp6{6}else{4},operation:Operation::Receive,source_port:42123,destination_port:52123,sent:4}),
            start:100,end:200,stats:Stats{status:Status::Matched,outbound:1,inbound:0,api:None},
        })}
    }
    #[test]fn confirmed_udp_timeout_retains_raw_code_and_requires_each_family(){
        for protocol in [Probe::Udp4,Probe::Udp6]{
            let mut info=timeout_info(protocol);let original=info;
            assert!(confirmed_blocked_udp(&info));assert!(!accepted_probe_exit(info.actual,20));
            assert_eq!(reported_socket_error(info.actual),Some(10060));
            let mut calls=0;assert_eq!(resolve_probe(&mut info,|observation|{calls+=1;observation.stats}),Ok(()));
            assert_eq!(calls,1);assert_eq!(info,original);
            assert!(failure_report("probe_outcome_mismatch",Some(64),Some(info)).starts_with("{\"ok\":false,"));
        }
    }
    #[test]fn every_timeout_evidence_field_is_required(){
        use kernel::{ApiFailure,ApiStage,Operation,Status};
        for index in 0..24{
            let mut info=timeout_info(Probe::Udp4);
            match index{
                0=>info.phase=ProbePhase::Before,1=>info.phase=ProbePhase::After,2=>info.protocol=Probe::Tcp4,3=>info.protocol=Probe::Tcp6,
                4=>info.actual=None,5=>info.actual=Some(10060),6=>info.actual=Some(0x10000|10013),7=>info.udp=None,
                8=>info.udp.as_mut().unwrap().record=None,
                9=>info.udp.as_mut().unwrap().record.as_mut().unwrap().family=6,
                10=>info.udp.as_mut().unwrap().record.as_mut().unwrap().source_port=0,
                11=>info.udp.as_mut().unwrap().record.as_mut().unwrap().destination_port=0,
                12=>info.udp.as_mut().unwrap().record.as_mut().unwrap().operation=Operation::Reply,
                13=>info.udp.as_mut().unwrap().record.as_mut().unwrap().operation=Operation::Complete,
                14=>info.udp.as_mut().unwrap().record.as_mut().unwrap().sent=3,
                15=>info.udp.as_mut().unwrap().record.as_mut().unwrap().sent=5,
                16=>info.udp.as_mut().unwrap().start=0,17=>info.udp.as_mut().unwrap().end=99,
                18=>info.udp.as_mut().unwrap().end=100_000_101,
                19=>info.udp.as_mut().unwrap().stats.outbound=0,
                20=>{let stats=&mut info.udp.as_mut().unwrap().stats;stats.outbound=0;stats.inbound=1;},
                21=>info.udp.as_mut().unwrap().stats.status=Status::Inconclusive,
                22=>info.udp.as_mut().unwrap().stats.api=Some(ApiFailure{stage:ApiStage::EnumPage,code:5}),
                23=>info.udp.as_mut().unwrap().stats.api=Some(ApiFailure{stage:ApiStage::EnumPage,code:0}),_=>unreachable!(),
            }
            assert!(!confirmed_blocked_udp(&info),"accepted mutation {index}");
        }
        for status in [Status::Unavailable,Status::Disabled,Status::Inconclusive,Status::Capped,Status::Deadline]{let mut info=timeout_info(Probe::Udp4);info.udp.as_mut().unwrap().stats.status=status;assert!(!confirmed_blocked_udp(&info));}
        for operation in [Operation::Bind,Operation::ReadTimeout,Operation::WriteTimeout,Operation::Connect,Operation::LocalAddress,Operation::Send,Operation::Reply,Operation::Complete]{let mut info=timeout_info(Probe::Udp6);info.udp.as_mut().unwrap().record.as_mut().unwrap().operation=operation;assert!(!confirmed_blocked_udp(&info));}
        let mut info=timeout_info(Probe::Udp6);info.udp.as_mut().unwrap().record.as_mut().unwrap().family=4;assert!(!confirmed_blocked_udp(&info));
    }
    #[test]fn every_other_tag_and_crash_remains_ineligible_for_kernel_confirmation(){
        let mut info=timeout_info(Probe::Udp4);
        for error in 1..=65535{info.actual=Some(0x10000|error);assert_eq!(confirmed_blocked_udp(&info),error==10060);}
        for actual in [None,Some(0),Some(20),Some(21),Some(124),Some(-1),Some(0xc0000005u32 as i32),Some(0x20001)]{info.actual=actual;assert!(!confirmed_blocked_udp(&info));}
    }
    #[test]fn resolver_uses_fresh_diagnostic_and_never_queries_other_phases_or_protocols(){
        use kernel::{Stats,Status};
        let mut info=timeout_info(Probe::Udp4);let mut calls=0;
        assert_eq!(resolve_probe(&mut info,|_|{calls+=1;Stats::empty(Status::Unavailable)}),Err("probe_outcome_mismatch"));
        assert_eq!(calls,1);assert_eq!(info.actual,Some(75596));assert_eq!(info.udp.unwrap().stats.status,Status::Unavailable);
        for protocol in PROBES{for phase in [ProbePhase::Before,ProbePhase::After]{
            let mut info=timeout_info(protocol);info.phase=phase;
            assert_eq!(resolve_probe(&mut info,|_|panic!("nonblocked diagnostic")),Err("probe_outcome_mismatch"));
            info.actual=Some(0);assert_eq!(resolve_probe(&mut info,|_|panic!("positive diagnostic")),Ok(()));
        }}
        for protocol in [Probe::Tcp4,Probe::Tcp6]{let mut info=timeout_info(protocol);assert_eq!(resolve_probe(&mut info,|_|panic!("TCP diagnostic")),Err("probe_outcome_mismatch"));}
        for protocol in PROBES{let mut info=timeout_info(protocol);info.actual=Some(20);assert_eq!(resolve_probe(&mut info,|_|panic!("explicit deny diagnostic")),Ok(()));}
    }
    fn fake_probe_cycle(missing:Option<Probe>,cleanup_failure:bool)->(Result<()>,Vec<(ProbePhase,Probe)>,Vec<String>){
        use kernel::{Stats,Status};
        let mut calls=Vec::new();let mut api=Fake::default();
        let before=run_probe_phase(ProbePhase::Before,|protocol,phase|{calls.push((phase,protocol));let mut info=timeout_info(protocol);info.phase=phase;info.actual=Some(0);resolve_probe(&mut info,|_|panic!("before diagnostic"))});
        assert_eq!(before,Ok(()));let rules=install(&mut api).unwrap();
        let blocked=run_probe_phase(ProbePhase::Blocked,|protocol,phase|{
            calls.push((phase,protocol));let mut info=timeout_info(protocol);if protocol.tcp(){info.actual=Some(20);}
            resolve_probe(&mut info,|observation|{
                let layer=kernel::wanted_layer(observation.record.unwrap().family,true).unwrap();assert_eq!(rules.iter().filter(|rule|rule.layer==layer).count(),1);
                if missing==Some(protocol){Stats::empty(Status::Inconclusive)}else{observation.stats}
            })
        });
        if cleanup_failure{api.fail=Some("delete");}let removed=cleanup(&mut api,&rules,true);
        let result=(||{blocked?;removed?;assert!(api.rules.is_empty());run_probe_phase(ProbePhase::After,|protocol,phase|{
            calls.push((phase,protocol));let mut info=timeout_info(protocol);info.phase=phase;info.actual=Some(0);resolve_probe(&mut info,|_|panic!("after diagnostic"))
        })})();(result,calls,api.events)
    }
    #[test]fn shared_phase_path_continues_both_confirmed_udp_probes_cleanup_and_all_after_positives(){
        let(result,calls,events)=fake_probe_cycle(None,false);assert_eq!(result,Ok(()));
        let expected:Vec<_>=[ProbePhase::Before,ProbePhase::Blocked,ProbePhase::After].into_iter().flat_map(|phase|PROBES.map(|protocol|(phase,protocol))).collect();
        assert_eq!(calls,expected);assert_eq!(events.iter().filter(|event|event.as_str()=="delete").count(),4);assert_eq!(events.last().unwrap(),"close");
    }
    #[test]fn missing_outbound_evidence_or_cleanup_failure_cannot_reach_after_positives(){
        for(missing,want_calls)in[(Some(Probe::Udp4),7),(Some(Probe::Udp6),8)]{
            let(result,calls,events)=fake_probe_cycle(missing,false);assert_eq!(result,Err("probe_outcome_mismatch"));assert_eq!(calls.len(),want_calls);
            assert!(!calls.iter().any(|(phase,_)|*phase==ProbePhase::After));assert_eq!(events.last().unwrap(),"close");
        }
        let(result,calls,events)=fake_probe_cycle(None,true);assert_eq!(result,Err("filter_cleanup_unverified"));assert_eq!(calls.len(),8);assert_eq!(events.last().unwrap(),"close");
    }
}
