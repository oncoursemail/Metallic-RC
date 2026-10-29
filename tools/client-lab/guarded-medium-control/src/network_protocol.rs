//! Pure, unwired MGM2 batch contract. Host observations are not wire authority.
//! Standalone: rustc --edition=2021 --test network_protocol.rs -o <temp-test-exe>

pub type Result<T> = std::result::Result<T, &'static str>;
pub const BYTES: usize = 128;
const PRESENCE: u32 = 0x13f;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stamp { pub pid: u32, pub created: u64 }
impl Stamp { fn valid(self) -> bool { self.pid != 0 && self.created != 0 } }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase { Before, Blocked, After }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Probe { Tcp4, Tcp6, Udp4, Udp6 }
const PROBES: [Probe; 4] = [Probe::Tcp4, Probe::Tcp6, Probe::Udp4, Probe::Udp6];
impl Probe {
    fn index(self) -> usize { PROBES.iter().position(|p| *p == self).unwrap() }
    fn family(self) -> u8 { match self { Self::Tcp4 | Self::Udp4 => 4, _ => 6 } }
    fn udp(self) -> bool { matches!(self, Self::Udp4 | Self::Udp6) }
}

#[cfg(test)]
mod tests {
    use super::*;
    const NONCE: [u8; 16] = [7; 16];
    const HELPER: Stamp = Stamp { pid: 99, created: 10 };
    const CLEAN: Cleanup = Cleanup { job_empty: true, drains_complete: true };
    const GUARD: GuardScope = GuardScope { filter_ids: [11,12,13,14], layer_ids: [21,22,23,24], exact_readback: true };
    fn root(id: u32) -> Root { Root { stamp: Stamp { pid: id, created: 100 + id as u64 }, port: 51000 + id as u16 } }
    fn frame(p: &Planner, kind: Kind, r: Option<Root>) -> Frame {
        let control = matches!(kind, Kind::Ready | Kind::Release | Kind::Released);
        Frame { kind, nonce: NONCE, seq: p.seq, phase: if control { None } else { Some(match p.stage { Stage::Before => Phase::Before, Stage::After => Phase::After, _ => Phase::Blocked }) },
            probe: if control { None } else { Some(PROBES[p.index]) }, root: r, raw_exit: 0, udp: None }
    }
    fn send(p: &mut Planner, f: Frame, host: Host) -> Result<Action> { p.receive(&f.encode()?, host, p.last_ms + 1) }
    fn positive(p: &mut Planner, id: u32) {
        let r = root(id); let f = frame(p, Kind::Positive, Some(r));
        send(p, f, Host::Positive { exit: ExitFacts { stamp: r.stamp, actual_exit: Some(0), observed_utc: 1000, drop: None }, cleanup: CLEAN, marker_exact: true }).unwrap();
    }
    fn prepared() -> Planner {
        let mut p = Planner::new(NONCE, 0).unwrap();
        for id in 1..=4 { positive(&mut p, id); }
        let f = frame(&p, Kind::Ready, None); send(&mut p, f, Host::Guard(GUARD, HELPER)).unwrap(); p
    }
    fn running(p: &mut Planner) -> Root {
        let r = root(10 + p.index as u32); let f = frame(p, Kind::Register, Some(r));
        send(p, f, Host::Admitted(r.stamp)).unwrap();
        let f = frame(p, Kind::Ack, Some(r)); assert_eq!(send(p, f, Host::AckUtc(1000)), Ok(Action::ResumeOnce(r))); r
    }
    fn result(p: &Planner, r: Root, udp: bool) -> (Frame, ExitFacts) {
        let mut f = frame(p, Kind::Result, Some(r)); f.raw_exit = if udp { 0x10000 | 10060 } else { 20 };
        let mut e = ExitFacts { stamp: r.stamp, actual_exit: Some(f.raw_exit), observed_utc: 1100, drop: None };
        if udp {
            f.udp = Some(UdpRecord { family: f.probe.unwrap().family(), operation: 6, sent: 4, source_port: 42000, destination_port: r.port });
            let slot = if f.probe.unwrap().family() == 4 { 0 } else { 1 };
            e.drop = Some(DropEvent { classify_drop: true, presence: PRESENCE, app_matches: true, family: f.probe.unwrap().family(), protocol: 17,
                local_loopback: true, remote_loopback: true, local_port: 42000, remote_port: r.port, timestamp: 1050,
                filter_id: GUARD.filter_ids[slot], layer_id: GUARD.layer_ids[slot], direction: 0x3901, is_loopback: true, complete: true, api_error: None });
        }
        (f, e)
    }
    fn finish_probe(p: &mut Planner) {
        let r = running(p); let (f,e) = result(p,r,p.index >= 2); send(p,f,Host::Exit(e)).unwrap();
        let f = frame(p, Kind::ProbeDone, Some(r)); send(p,f,Host::Cleanup(CLEAN)).unwrap();
        let f = frame(p, Kind::Next, Some(r)); send(p,f,Host::None).unwrap();
    }
    fn release_ready() -> Planner { let mut p = prepared(); for _ in 0..4 { finish_probe(&mut p); } p }
    fn removal() -> Removal { Removal { filter_ids: GUARD.filter_ids, all_absent: true, sublayer_deleted: true, session_closed: true, helper: HELPER, helper_exit: Some(0) } }

    #[test] fn complete_batch_requires_twelve_controls_and_all_after() {
        let mut p = release_ready(); assert_eq!(p.seq,25);
        let f = frame(&p,Kind::Release,None); assert_eq!(send(&mut p,f,Host::None),Ok(Action::RemoveGuards));
        let f = frame(&p,Kind::Released,None); assert_eq!(send(&mut p,f,Host::Removed(removal())),Ok(Action::BeginAfter));
        for id in 21..=23 { positive(&mut p,id); assert_ne!(p.stage(),Stage::Complete); }
        positive(&mut p,24); assert_eq!(p.stage(),Stage::Complete); assert_eq!(p.next_sequence(),31);
        assert!(!p.recovery().launcher_exit_required);
    }
    #[test] fn codec_all_kinds_and_crash_bits_roundtrip() {
        let p = prepared(); let r = root(10);
        for kind in [Kind::Positive,Kind::Ready,Kind::Register,Kind::Ack,Kind::Result,Kind::ProbeDone,Kind::Next,Kind::Release,Kind::Released] {
            let mut f = frame(&p,kind,if matches!(kind,Kind::Ready|Kind::Release|Kind::Released) { None } else { Some(r) });
            if kind == Kind::Positive { f.phase=Some(Phase::Before); }
            if kind == Kind::Result { f.raw_exit=0xc0000005u32 as i32; }
            assert_eq!(Frame::decode(&f.encode().unwrap()),Ok(f));
        }
        let mut p=prepared(); finish_probe(&mut p); finish_probe(&mut p); let r=running(&mut p); let(f,_)=result(&p,r,true);
        assert_eq!(Frame::decode(&f.encode().unwrap()),Ok(f));
    }
    #[test] fn codec_refuses_lengths_reserved_tags_and_zero_identity() {
        let p=prepared(); let bytes=frame(&p,Kind::Register,Some(root(10))).encode().unwrap();
        for n in 0..128 { assert!(Frame::decode(&bytes[..n]).is_err()); }
        let mut long=bytes.to_vec(); long.push(0); assert!(Frame::decode(&long).is_err());
        for (offset,value) in [(0,0),(4,3),(5,0),(6,3),(7,4),(42,2),(43,1),(80,1),(127,1)] {
            let mut b=bytes; b[offset]=value; assert!(Frame::decode(&b).is_err(),"offset {offset}");
        }
        let mut b=bytes;b[8..24].fill(0);assert!(Frame::decode(&b).is_err());
        let mut b=bytes;b[24..28].copy_from_slice(&31u32.to_le_bytes());assert!(Frame::decode(&b).is_err());
        let mut b=bytes;b[28..32].fill(0);assert!(Frame::decode(&b).is_err());
        let mut b=bytes;b[40..42].fill(0);assert!(Frame::decode(&b).is_err());
    }
    #[test] fn udp_codec_refuses_wrong_binding_and_hidden_bytes() {
        let mut p=prepared();finish_probe(&mut p);finish_probe(&mut p);let r=running(&mut p);let(f,_)=result(&p,r,true);let b=f.encode().unwrap();
        for (offset,value) in [(48,0),(52,6),(53,9),(54,5),(55,1),(56,0),(58,0),(60,1),(79,1)] {
            let mut x=b; x[offset]=value; if offset==56 {x[57]=0;} if offset==58{x[59]=0;} assert!(Frame::decode(&x).is_err(),"offset {offset}");
        }
        let mut x=b;x[42]=0;assert!(Frame::decode(&x).is_err());
        let mut f=f;f.probe=Some(Probe::Tcp4);assert!(f.encode().is_err());
    }
    #[test] fn order_nonce_replay_and_early_release_fail_closed() {
        for mode in 0..5 {
            let mut p=prepared();let mut f=frame(&p,Kind::Register,Some(root(10)));
            match mode {0=>f.seq-=1,1=>f.seq+=1,2=>f.nonce[0]^=1,3=>f.probe=Some(Probe::Tcp6),_=>f=frame(&p,Kind::Release,None)}
            assert!(send(&mut p,f,Host::Admitted(root(10).stamp)).is_err());assert_eq!(p.stage(),Stage::Failed);
        }
    }
    #[test] fn ack_once_and_result_never_resume_before_admission() {
        let mut p=prepared();let r=running(&mut p);let f=frame(&p,Kind::Ack,Some(r));
        assert!(send(&mut p,f,Host::AckUtc(1000)).is_err());assert_eq!(p.recovery().retained[0],Some(r.stamp));
        let mut p=prepared();let f=frame(&p,Kind::Result,Some(root(10)));assert!(send(&mut p,f,Host::None).is_err());
    }
    #[test] fn actual_exit_and_stamp_are_required() {
        for mode in 0..4 {let mut p=prepared();let r=running(&mut p);let(f,mut e)=result(&p,r,false);
            match mode {0=>e.actual_exit=None,1=>e.actual_exit=Some(0),2=>e.stamp.created+=1,_=>e.stamp.pid+=1}
            assert!(send(&mut p,f,Host::Exit(e)).is_err());}
    }
    #[test] fn timeout_alone_unknown_bad_reply_and_crash_fail() {
        for code in [0,21,10060,0x10000|10060,0x10000|10013,0xc0000005u32 as i32] {
            let mut p=prepared();let r=running(&mut p);let(mut f,mut e)=result(&p,r,false);f.raw_exit=code;e.actual_exit=Some(code);
            assert!(send(&mut p,f,Host::Exit(e)).is_err());
        }
    }
    #[test] fn every_drop_scope_field_is_required() {
        for mode in 0..24 {let mut p=prepared();finish_probe(&mut p);finish_probe(&mut p);let r=running(&mut p);let(f,mut e)=result(&p,r,true);let d=e.drop.as_mut().unwrap();
            match mode {0=>d.classify_drop=false,1=>d.presence=0,2=>d.app_matches=false,3=>d.family=6,4=>d.protocol=6,5=>d.local_loopback=false,6=>d.remote_loopback=false,
                7=>d.local_port+=1,8=>d.remote_port+=1,9=>d.timestamp=999,10=>d.timestamp=1101,11=>d.filter_id=13,12=>d.layer_id=23,13=>d.direction=0x3900,
                14=>d.is_loopback=false,15=>d.complete=false,16=>d.api_error=Some(0),17=>d.api_error=Some(5),18=>{d.local_port=r.port;d.remote_port=42000;},
                19=>e.drop=None,20=>e.observed_utc=999,21=>e.observed_utc=100_001_001,22=>d.filter_id=999,_=>d.layer_id=999}
            assert!(send(&mut p,f,Host::Exit(e)).is_err(),"mutation {mode}");
        }
        for bit in [0,1,2,3,4,5,8] {let mut p=prepared();finish_probe(&mut p);finish_probe(&mut p);let r=running(&mut p);let(f,mut e)=result(&p,r,true);e.drop.as_mut().unwrap().presence&=!(1<<bit);assert!(send(&mut p,f,Host::Exit(e)).is_err());}
    }
    #[test] fn udp_receive_and_full_send_required() {
        for mode in 0..4 {let mut p=prepared();finish_probe(&mut p);finish_probe(&mut p);let r=running(&mut p);let(mut f,e)=result(&p,r,true);
            match mode {0=>f.udp.as_mut().unwrap().operation=5,1=>f.udp.as_mut().unwrap().operation=7,2=>f.udp.as_mut().unwrap().sent=3,_=>f.udp=None}
            assert!(send(&mut p,f,Host::Exit(e)).is_err());}
    }
    #[test] fn all_guard_fields_and_positive_facts_required() {
        for mode in 0..5 {let mut p=Planner::new(NONCE,0).unwrap();for id in 1..=4{positive(&mut p,id);}let mut g=GUARD;
            match mode {0=>g.exact_readback=false,1=>g.filter_ids[0]=0,2=>g.layer_ids[0]=0,3=>g.filter_ids[1]=g.filter_ids[0],_=>g.layer_ids[1]=g.layer_ids[0]}
            let f=frame(&p,Kind::Ready,None);assert!(send(&mut p,f,Host::Guard(g, HELPER)).is_err());}
        for mode in 0..4 {let mut p=Planner::new(NONCE,0).unwrap();let r=root(1);let f=frame(&p,Kind::Positive,Some(r));let mut cleanup=CLEAN;let mut exit=ExitFacts{stamp:r.stamp,actual_exit:Some(0),observed_utc:1000,drop:None};
            if mode==0{cleanup.job_empty=false;}if mode==1{cleanup.drains_complete=false;}if mode==2{exit.actual_exit=None;}
            assert!(send(&mut p,f,Host::Positive{exit,cleanup,marker_exact:mode!=3}).is_err());}
    }
    #[test] fn no_release_without_cleanup_and_exact_removal() {
        for mode in 0..7 {let mut p=release_ready();let f=frame(&p,Kind::Release,None);send(&mut p,f,Host::None).unwrap();let mut proof=removal();
            match mode{0=>proof.filter_ids[0]+=1,1=>proof.all_absent=false,2=>proof.sublayer_deleted=false,3=>proof.session_closed=false,4=>proof.helper.pid+=1,5=>proof.helper_exit=None,_=>proof.helper_exit=Some(1)}
            let f=frame(&p,Kind::Released,None);assert!(send(&mut p,f,Host::Removed(proof)).is_err());}
        let mut p=prepared();let r=running(&mut p);let(f,e)=result(&p,r,false);send(&mut p,f,Host::Exit(e)).unwrap();let f=frame(&p,Kind::Next,Some(r));assert!(send(&mut p,f,Host::None).is_err());
    }
    #[test] fn recovery_keeps_completed_and_current_roots_on_lost_next_register() {
        let mut p=prepared();finish_probe(&mut p);let f=frame(&p,Kind::Register,Some(root(11)));send(&mut p,f,Host::Admitted(root(11).stamp)).unwrap();p.abort();
        assert_eq!(p.recovery().retained,[Some(root(10).stamp),Some(root(11).stamp),None,None]);
        assert!(!p.recovery_complete(None,&[root(10).stamp,root(11).stamp]));
        assert!(!p.recovery_complete(Some(false),&[root(10).stamp,root(11).stamp]));
        assert!(!p.recovery_complete(Some(true),&[root(10).stamp]));
        assert!(p.recovery_complete(Some(true),&[root(10).stamp,root(11).stamp]));
    }
    #[test] fn identity_reuse_and_wrong_ack_clock_fail() {
        let mut p=prepared();let mut f=frame(&p,Kind::Register,Some(root(1)));assert!(send(&mut p,f,Host::Admitted(root(1).stamp)).is_err());
        let mut p=prepared();f=frame(&p,Kind::Register,Some(root(10)));send(&mut p,f,Host::Admitted(root(10).stamp)).unwrap();let f=frame(&p,Kind::Ack,Some(root(10)));assert!(send(&mut p,f,Host::AckUtc(109)).is_err());
    }
    #[test] fn clocks_and_sticky_failure_are_bounded() {
        for (start,now) in [(100,99),(0,30000)] {let mut p=Planner::new(NONCE,start).unwrap();let f=frame(&p,Kind::Positive,Some(root(1)));assert!(p.receive(&f.encode().unwrap(),Host::None,now).is_err());assert_eq!(p.stage(),Stage::Failed);}
        let mut p=prepared();let f=frame(&p,Kind::Register,Some(root(10)));assert!(p.receive(&f.encode().unwrap(),Host::Admitted(root(10).stamp),90000).is_err());
        assert!(p.receive(&f.encode().unwrap(),Host::Admitted(root(10).stamp),10).is_err());
        assert!(Planner::new([0;16],0).is_err());
    }
    #[test] fn helper_is_bound_once_only_after_four_before_controls() {
        let mut p=Planner::new(NONCE,0).unwrap(); assert_eq!(p.helper,None);
        for id in 1..=4 { positive(&mut p,id); assert_eq!(p.helper,None); }
        assert_eq!(p.stage(),Stage::NeedGuards);assert_eq!(p.next_sequence(),4);
        let f=frame(&p,Kind::Ready,None);send(&mut p,f,Host::Guard(GUARD,HELPER)).unwrap();
        assert_eq!(p.helper,Some(HELPER));assert_eq!(p.next_sequence(),5);
        let f=frame(&p,Kind::Ready,None);assert!(send(&mut p,f,Host::Guard(GUARD,Stamp{pid:100,created:11})).is_err());
        assert_eq!(p.helper,Some(HELPER));
    }
    #[test] fn premature_missing_invalid_and_replayed_helper_admission_refuse() {
        for count in 0..4 {let mut p=Planner::new(NONCE,0).unwrap();for id in 1..=count{positive(&mut p,id);}
            let f=frame(&p,Kind::Ready,None);assert!(send(&mut p,f,Host::Guard(GUARD,HELPER)).is_err());assert_eq!(p.helper,None);}
        for host in [Host::None,Host::Guard(GUARD,Stamp{pid:0,created:1}),Host::Guard(GUARD,Stamp{pid:1,created:0})] {
            let mut p=Planner::new(NONCE,0).unwrap();for id in 1..=4{positive(&mut p,id);}let f=frame(&p,Kind::Ready,None);
            assert!(send(&mut p,f,host).is_err());assert_eq!(p.helper,None);
        }
        let mut p=Planner::new(NONCE,0).unwrap();for id in 1..=4{positive(&mut p,id);}let f=frame(&p,Kind::Ready,None);
        send(&mut p,f,Host::Guard(GUARD,HELPER)).unwrap();assert!(send(&mut p,f,Host::Guard(GUARD,HELPER)).is_err());
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Root { pub stamp: Stamp, pub port: u16 }
impl Root { fn valid(self) -> bool { self.stamp.valid() && self.port != 0 } }

/// Same closed MRU1 scalar fields as the fixture's 32-byte UDP record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UdpRecord {
    pub family: u8, pub operation: u8, pub sent: u8,
    pub source_port: u16, pub destination_port: u16,
}
impl UdpRecord {
    fn valid(self, probe: Probe, root: Root) -> bool {
        probe.udp() && self.family == probe.family() && self.operation <= 8 && self.sent <= 4 &&
        self.source_port != 0 && self.destination_port == root.port &&
        (self.sent == 0 || self.operation >= 5)
    }
    fn encode(self) -> [u8; 32] {
        let mut b = [0; 32]; b[..4].copy_from_slice(b"MRU1");
        b[4] = self.family; b[5] = self.operation; b[6] = self.sent;
        b[8..10].copy_from_slice(&self.source_port.to_le_bytes());
        b[10..12].copy_from_slice(&self.destination_port.to_le_bytes()); b
    }
    fn decode(b: &[u8], probe: Probe, root: Root) -> Result<Self> {
        if b.len() != 32 || &b[..4] != b"MRU1" || b[7] != 0 || b[12..].iter().any(|x| *x != 0) {
            return Err("udp_record_shape");
        }
        let value = Self { family: b[4], operation: b[5], sent: b[6],
            source_port: u16::from_le_bytes(b[8..10].try_into().unwrap()),
            destination_port: u16::from_le_bytes(b[10..12].try_into().unwrap()) };
        if !value.valid(probe, root) { return Err("udp_record_binding"); } Ok(value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind { Positive = 1, Ready = 2, Register = 3, Ack = 4, Result = 5,
                ProbeDone = 6, Next = 7, Release = 8, Released = 9 }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Frame {
    pub kind: Kind, pub nonce: [u8; 16], pub seq: u32,
    pub phase: Option<Phase>, pub probe: Option<Probe>, pub root: Option<Root>,
    pub raw_exit: i32, pub udp: Option<UdpRecord>,
}
impl Frame {
    fn validate(self) -> Result<()> {
        if self.nonce == [0; 16] || self.seq > 30 { return Err("frame_identity"); }
        let control = matches!(self.kind, Kind::Ready | Kind::Release | Kind::Released);
        if control {
            if self.phase.is_some() || self.probe.is_some() || self.root.is_some() || self.raw_exit != 0 || self.udp.is_some() {
                return Err("control_shape");
            }
        } else {
            let root = self.root.ok_or("root_required")?;
            let probe = self.probe.ok_or("probe_required")?;
            if !root.valid() { return Err("root_identity"); }
            if self.kind == Kind::Positive {
                if !matches!(self.phase, Some(Phase::Before | Phase::After)) || self.udp.is_some() { return Err("positive_shape"); }
            } else if self.phase != Some(Phase::Blocked) { return Err("blocked_phase_required"); }
            if !matches!(self.kind, Kind::Positive | Kind::Result) && (self.raw_exit != 0 || self.udp.is_some()) {
                return Err("root_control_shape");
            }
            if let Some(record) = self.udp { if !record.valid(probe, root) { return Err("udp_record_binding"); } }
        }
        Ok(())
    }
    pub fn encode(self) -> Result<[u8; BYTES]> {
        self.validate()?;
        let mut b = [0; BYTES]; b[..4].copy_from_slice(b"MGM2"); b[4] = 2; b[5] = self.kind as u8;
        b[6] = match self.phase { Some(Phase::Before) => 0, Some(Phase::Blocked) => 1, Some(Phase::After) => 2, None => 255 };
        b[7] = self.probe.map(|p| p.index() as u8).unwrap_or(255);
        b[8..24].copy_from_slice(&self.nonce); b[24..28].copy_from_slice(&self.seq.to_le_bytes());
        if let Some(root) = self.root {
            b[28..32].copy_from_slice(&root.stamp.pid.to_le_bytes());
            b[32..40].copy_from_slice(&root.stamp.created.to_le_bytes());
            b[40..42].copy_from_slice(&root.port.to_le_bytes());
        }
        b[44..48].copy_from_slice(&self.raw_exit.to_le_bytes());
        if let Some(record) = self.udp { b[42] = 1; b[48..80].copy_from_slice(&record.encode()); }
        Ok(b)
    }
    pub fn decode(b: &[u8]) -> Result<Self> {
        if b.len() != BYTES || &b[..4] != b"MGM2" || b[4] != 2 || b[43] != 0 || b[80..].iter().any(|x| *x != 0) {
            return Err("closed_frame_required");
        }
        let kind = match b[5] { 1 => Kind::Positive, 2 => Kind::Ready, 3 => Kind::Register, 4 => Kind::Ack,
            5 => Kind::Result, 6 => Kind::ProbeDone, 7 => Kind::Next, 8 => Kind::Release, 9 => Kind::Released, _ => return Err("closed_kind_required") };
        let phase = match b[6] { 0 => Some(Phase::Before), 1 => Some(Phase::Blocked), 2 => Some(Phase::After), 255 => None, _ => return Err("closed_phase_required") };
        let probe = match b[7] { 0..=3 => Some(PROBES[b[7] as usize]), 255 => None, _ => return Err("closed_probe_required") };
        let stamp = Stamp { pid: u32::from_le_bytes(b[28..32].try_into().unwrap()), created: u64::from_le_bytes(b[32..40].try_into().unwrap()) };
        let port = u16::from_le_bytes(b[40..42].try_into().unwrap());
        let root = if stamp == (Stamp { pid: 0, created: 0 }) && port == 0 { None } else { Some(Root { stamp, port }) };
        let udp = match b[42] {
            0 => { if b[48..80].iter().any(|x| *x != 0) { return Err("unexpected_udp_bytes"); } None },
            1 => Some(UdpRecord::decode(&b[48..80], probe.ok_or("probe_required")?, root.ok_or("root_required")?)?),
            _ => return Err("closed_metadata_flag"),
        };
        let frame = Self { kind, nonce: b[8..24].try_into().unwrap(), seq: u32::from_le_bytes(b[24..28].try_into().unwrap()),
            phase, probe, root, raw_exit: i32::from_le_bytes(b[44..48].try_into().unwrap()), udp };
        frame.validate()?; Ok(frame)
    }
}

// These facts are supplied by trusted host adapters, NEVER decoded from frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GuardScope { pub filter_ids: [u64; 4], pub layer_ids: [u16; 4], pub exact_readback: bool }
impl GuardScope {
    fn valid(self) -> bool {
        self.exact_readback && self.filter_ids.iter().all(|x| *x != 0) && self.layer_ids.iter().all(|x| *x != 0) &&
        (0..4).all(|i| (0..i).all(|j| self.filter_ids[i] != self.filter_ids[j] && self.layer_ids[i] != self.layer_ids[j]))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cleanup { pub job_empty: bool, pub drains_complete: bool }
impl Cleanup { fn verified(self) -> bool { self.job_empty && self.drains_complete } }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DropEvent {
    pub classify_drop: bool, pub presence: u32, pub app_matches: bool,
    pub family: u8, pub protocol: u8, pub local_loopback: bool, pub remote_loopback: bool,
    pub local_port: u16, pub remote_port: u16, pub timestamp: u64,
    pub filter_id: u64, pub layer_id: u16, pub direction: u32, pub is_loopback: bool,
    pub complete: bool, pub api_error: Option<u32>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExitFacts { pub stamp: Stamp, pub actual_exit: Option<i32>, pub observed_utc: u64, pub drop: Option<DropEvent> }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Removal { pub filter_ids: [u64; 4], pub all_absent: bool, pub sublayer_deleted: bool,
                     pub session_closed: bool, pub helper: Stamp, pub helper_exit: Option<i32> }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Host {
    None, Positive { exit: ExitFacts, cleanup: Cleanup, marker_exact: bool },
    Guard(GuardScope, Stamp), Admitted(Stamp), AckUtc(u64), Exit(ExitFacts), Cleanup(Cleanup), Removed(Removal),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome { AccessDenied, OutboundUdpDrop }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage { Before, NeedGuards, Register, Ack, Running, ProbeDone, Next, Release, Released, After, Complete, Failed }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action { Wait, ResumeOnce(Root), ProbeAccepted(Outcome), RemoveGuards, BeginAfter, Complete }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Recovery { pub launcher_exit_required: bool, pub retained: [Option<Stamp>; 4] }

pub struct Planner {
    nonce: [u8; 16], helper: Option<Stamp>, seq: u32, stage: Stage, index: usize,
    start_ms: u64, last_ms: u64, guard: Option<GuardScope>, current: Option<Root>, ack_utc: u64,
    retained: [Option<Stamp>; 4], outcomes: [Option<Outcome>; 4], cleaned: [bool; 4],
    seen: [Option<Stamp>; 12], seen_count: usize,
}
impl Planner {
    pub fn new(nonce: [u8; 16], start_ms: u64) -> Result<Self> {
        if nonce == [0; 16] { return Err("session_identity_required"); }
        Ok(Self { nonce, helper: None, seq: 0, stage: Stage::Before, index: 0, start_ms, last_ms: start_ms,
            guard: None, current: None, ack_utc: 0, retained: [None; 4], outcomes: [None; 4],
            cleaned: [false; 4], seen: [None; 12], seen_count: 0 })
    }
    pub fn stage(&self) -> Stage { self.stage }
    pub fn next_sequence(&self) -> u32 { self.seq }
    pub fn abort(&mut self) { self.stage = Stage::Failed; }
    pub fn recovery(&self) -> Recovery {
        Recovery { launcher_exit_required: self.stage != Stage::Complete, retained: self.retained }
    }
    pub fn recovery_complete(&self, launcher_exited: Option<bool>, exited: &[Stamp]) -> bool {
        self.stage == Stage::Failed && launcher_exited == Some(true) &&
        self.retained.iter().flatten().all(|stamp| exited.contains(stamp))
    }
    pub fn receive(&mut self, bytes: &[u8], host: Host, now_ms: u64) -> Result<Action> {
        let result = (|| {
            if self.stage == Stage::Failed || now_ms < self.last_ms || now_ms < self.start_ms ||
                now_ms-self.start_ms >= 90_000 { return Err("session_deadline_or_failure"); }
            if matches!(self.stage, Stage::Before | Stage::NeedGuards) && now_ms-self.start_ms >= 30_000 {
                return Err("startup_deadline");
            }
            let frame = Frame::decode(bytes)?;
            if frame.nonce != self.nonce || frame.seq != self.seq { return Err("nonce_or_sequence"); }
            let action = self.apply(frame, host)?;
            self.seq += 1; self.last_ms = now_ms; Ok(action)
        })();
        if result.is_err() { self.abort(); } result
    }
    fn root_matches(&self, f: Frame) -> bool {
        f.phase == Some(Phase::Blocked) && f.probe == Some(PROBES[self.index]) && f.root == self.current
    }
    fn remember(&mut self, stamp: Stamp) -> Result<()> {
        if self.seen.contains(&Some(stamp)) || self.seen_count == 12 { return Err("root_replay"); }
        self.seen[self.seen_count] = Some(stamp); self.seen_count += 1; Ok(())
    }
    fn apply(&mut self, f: Frame, host: Host) -> Result<Action> {
        if f.kind == Kind::Positive && matches!(self.stage, Stage::Before | Stage::After) {
            let phase = if self.stage == Stage::Before { Phase::Before } else { Phase::After };
            let Host::Positive { exit, cleanup, marker_exact } = host else { return Err("positive_host_required"); };
            let root = f.root.ok_or("root_required")?;
            if f.phase != Some(phase) || f.probe != Some(PROBES[self.index]) || f.raw_exit != 0 ||
                exit.stamp != root.stamp || exit.actual_exit != Some(0) || !cleanup.verified() || !marker_exact {
                return Err("positive_not_verified");
            }
            self.remember(root.stamp)?; self.index += 1;
            if self.index == 4 {
                self.index = 0;
                self.stage = if phase == Phase::Before { Stage::NeedGuards } else { Stage::Complete };
            }
            return Ok(if self.stage == Stage::Complete { Action::Complete } else { Action::Wait });
        }
        match (self.stage, f.kind, host) {
            (Stage::NeedGuards, Kind::Ready, Host::Guard(scope, helper)) if scope.valid() && helper.valid() && self.helper.is_none() => {
                self.helper = Some(helper); self.guard = Some(scope); self.stage = Stage::Register; Ok(Action::Wait)
            }
            (Stage::Register, Kind::Register, Host::Admitted(stamp)) => {
                let root = f.root.ok_or("root_required")?;
                if f.probe != Some(PROBES[self.index]) || stamp != root.stamp { return Err("root_admission"); }
                self.remember(stamp)?;
                // The adapter must retain its real handle before supplying Admitted.
                self.retained[self.index] = Some(stamp); self.current = Some(root); self.stage = Stage::Ack; Ok(Action::Wait)
            }
            (Stage::Ack, Kind::Ack, Host::AckUtc(utc)) if self.root_matches(f) => {
                if utc < self.current.unwrap().stamp.created { return Err("ack_clock"); }
                self.ack_utc = utc; self.stage = Stage::Running; Ok(Action::ResumeOnce(self.current.unwrap()))
            }
            (Stage::Running, Kind::Result, Host::Exit(exit)) if self.root_matches(f) => {
                if exit.stamp != self.current.unwrap().stamp || exit.actual_exit != Some(f.raw_exit) { return Err("retained_exit_mismatch"); }
                let outcome = if f.raw_exit == 20 { Outcome::AccessDenied }
                    else if self.udp_confirmed(f, exit) { Outcome::OutboundUdpDrop }
                    else { return Err("blocked_result_not_verified"); };
                self.outcomes[self.index] = Some(outcome); self.stage = Stage::ProbeDone; Ok(Action::ProbeAccepted(outcome))
            }
            (Stage::ProbeDone, Kind::ProbeDone, Host::Cleanup(cleanup)) if self.root_matches(f) && cleanup.verified() => {
                self.cleaned[self.index] = true; self.stage = Stage::Next; Ok(Action::Wait)
            }
            (Stage::Next, Kind::Next, Host::None) if self.root_matches(f) => {
                self.current = None;
                if self.index == 3 { self.stage = Stage::Release; }
                else { self.index += 1; self.stage = Stage::Register; } Ok(Action::Wait)
            }
            (Stage::Release, Kind::Release, Host::None) if self.outcomes.iter().all(Option::is_some) && self.cleaned.iter().all(|v| *v) => {
                self.stage = Stage::Released; Ok(Action::RemoveGuards)
            }
            (Stage::Released, Kind::Released, Host::Removed(proof)) => {
                if proof.filter_ids != self.guard.ok_or("guard_required")?.filter_ids || !proof.all_absent ||
                    !proof.sublayer_deleted || !proof.session_closed || Some(proof.helper) != self.helper || proof.helper_exit != Some(0) {
                    return Err("guard_removal_unverified");
                }
                self.index = 0; self.stage = Stage::After; Ok(Action::BeginAfter)
            }
            _ => Err("batch_state_refused"),
        }
    }
    fn udp_confirmed(&self, f: Frame, exit: ExitFacts) -> bool {
        if f.raw_exit != (0x10000 | 10060) || !f.probe.unwrap().udp() { return false; }
        let (Some(record), Some(event), Some(scope)) = (f.udp, exit.drop, self.guard) else { return false; };
        let family = f.probe.unwrap().family(); let slot = if family == 4 { 0 } else { 1 };
        record.operation == 6 && record.sent == 4 && self.ack_utc != 0 && exit.observed_utc >= self.ack_utc &&
        exit.observed_utc-self.ack_utc <= 100_000_000 && event.classify_drop && event.presence & PRESENCE == PRESENCE &&
        event.app_matches && event.family == family && event.protocol == 17 && event.local_loopback && event.remote_loopback &&
        event.local_port == record.source_port && event.remote_port == record.destination_port &&
        event.timestamp >= self.ack_utc && event.timestamp <= exit.observed_utc &&
        event.filter_id == scope.filter_ids[slot] && event.layer_id == scope.layer_ids[slot] &&
        event.direction == 0x3901 && event.is_loopback && event.complete && event.api_error.is_none()
    }
}
