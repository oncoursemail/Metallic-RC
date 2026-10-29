//! Fixture policy and lifecycle. Unit tests use only the fake API below.
pub type Result<T> = std::result::Result<T, &'static str>;
use std::{sync::{Arc,atomic::{AtomicBool,Ordering}},time::{Duration,Instant}};
pub const REQUESTED_FILTER_FLAGS:u32=0;

/// Failure-only, bounded numeric metadata. No observed path/blob is accepted.
pub fn failure_report(error:&'static str,observed_flags:Option<u32>)->String {
    let numeric=match observed_flags {
        Some(flags)=>format!(",\"requestedFlags\":{REQUESTED_FILTER_FLAGS},\"observedFlags\":{flags}"),
        None=>String::new(),
    };
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
    match exchange{Ok(true)=>0,Err(Some(10013))=>20,_=>21}
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
    struct Fake { events: Vec<String>, rules: Vec<Rule>, fail: Option<&'static str>, mismatch: bool, wrong_field:u8 }
    impl Fake {
        fn event(&mut self, name: &str) -> Result<()> { self.events.push(name.into()); if self.fail == Some(name) { Err("injected") } else { Ok(()) } }
    }
    impl Api for Fake {
        fn open_dynamic(&mut self) -> Result<()> { self.event("open_dynamic") }
        fn begin(&mut self) -> Result<()> { self.event("begin") }
        fn add_sublayer(&mut self) -> Result<()> { self.event("sublayer") }
        fn add(&mut self, layer: Layer) -> Result<Rule> {
            self.event(&format!("add_{}", self.rules.len()))?;
            let r = Rule { id: self.rules.len() as u64+1, key: [self.rules.len() as u8;16], sublayer:[9;16], layer, app_id: vec![4,2], block:true, flags:REQUESTED_FILTER_FLAGS, conditions:1, app_equal:true };
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
        for error in [Some(10060),Some(10061),Some(10035),Some(5),None]{assert_eq!(classify_probe(Err(error)),21);}
        assert_eq!(classify_probe(Ok(false)),21);assert_eq!(classify_probe(Ok(true)),0);
    }
    #[test] fn unexpected_or_crash_child_exits_never_pass(){
        assert!(accepted_probe_exit(Some(0),0));assert!(accepted_probe_exit(Some(20),20));
        for code in [None,Some(1),Some(21),Some(124),Some(0xc0000005u32 as i32)]{assert!(!accepted_probe_exit(code,20));assert!(!accepted_probe_exit(code,0));}
        assert!(!accepted_probe_exit(Some(21),21));assert!(!accepted_probe_exit(Some(0),20));assert!(!accepted_probe_exit(Some(20),0));
    }
    #[test] fn failure_without_observation_preserves_original_schema(){
        assert_eq!(REQUESTED_FILTER_FLAGS,0);
        assert_eq!(failure_report("readback_missing",None),"{\"ok\":false,\"error\":\"readback_missing\",\"cleanupVerified\":false,\"originalClientTested\":false,\"executionReady\":false}");
    }
    #[test] fn numeric_failure_metadata_is_bounded_unsigned_and_never_success(){
        for (flags,decimal) in [(0,"0"),(64,"64"),(u32::MAX,"4294967295")] {
            let report=failure_report("readback_flags_mismatch",Some(flags));
            assert_eq!(report,format!("{{\"ok\":false,\"error\":\"readback_flags_mismatch\",\"cleanupVerified\":false,\"originalClientTested\":false,\"executionReady\":false,\"requestedFlags\":0,\"observedFlags\":{decimal}}}"));
            assert!(report.len()<2048);
        }
    }
}
