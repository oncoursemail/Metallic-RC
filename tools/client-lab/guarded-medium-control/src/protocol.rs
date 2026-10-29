use crate::Result;
pub const BYTES:usize=64;
#[derive(Clone,Copy,Debug,PartialEq,Eq)]pub struct Stamp{pub pid:u32,pub created:u64}
impl Stamp{pub fn valid(self)->bool{self.pid!=0&&self.created!=0}}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]pub enum Tag{Hello=1,Ready=2,Register=3,Ack=4,Done=5,Released=6,Cancel=7}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]pub struct Frame{pub tag:Tag,pub nonce:[u8;16],pub seq:u32,pub stamp:Stamp,pub status:u32}
impl Frame{
 pub fn new(tag:Tag,nonce:[u8;16],seq:u32,stamp:Stamp,status:u32)->Self{Self{tag,nonce,seq,stamp,status}}
 pub fn encode(self)->[u8;BYTES]{let mut b=[0;BYTES];b[..4].copy_from_slice(b"MGM1");b[4]=1;b[5]=self.tag as u8;b[8..24].copy_from_slice(&self.nonce);b[24..28].copy_from_slice(&self.seq.to_le_bytes());b[28..32].copy_from_slice(&self.stamp.pid.to_le_bytes());b[32..40].copy_from_slice(&self.stamp.created.to_le_bytes());b[40..44].copy_from_slice(&self.status.to_le_bytes());b}
 pub fn decode(b:&[u8])->Result<Self>{
  if b.len()!=BYTES||&b[..4]!=b"MGM1"||b[4]!=1||b[6..8].iter().chain(b[44..].iter()).any(|v|*v!=0){return Err("closed_frame_required")}
  let tag=match b[5]{1=>Tag::Hello,2=>Tag::Ready,3=>Tag::Register,4=>Tag::Ack,5=>Tag::Done,6=>Tag::Released,7=>Tag::Cancel,_=>return Err("closed_tag_required")};
  let f=Self{tag,nonce:b[8..24].try_into().unwrap(),seq:u32::from_le_bytes(b[24..28].try_into().unwrap()),stamp:Stamp{pid:u32::from_le_bytes(b[28..32].try_into().unwrap()),created:u64::from_le_bytes(b[32..40].try_into().unwrap())},status:u32::from_le_bytes(b[40..44].try_into().unwrap())};
  let empty=f.stamp==Stamp{pid:0,created:0};
  let shape=match tag{Tag::Hello=>f.seq==0&&f.stamp.valid()&&f.status==0,Tag::Ready=>f.seq==1&&empty&&f.status==4,Tag::Register=>f.seq==2&&f.stamp.valid()&&f.status==0,Tag::Ack=>f.seq==3&&f.stamp.valid()&&f.status==0,Tag::Done=>f.seq==4&&f.stamp.valid()&&f.status==0,Tag::Released=>f.seq==5&&f.stamp.valid()&&f.status==0,Tag::Cancel=>(f.seq==2&&empty||f.seq==4&&f.stamp.valid())&&f.status==0};
  if !shape||f.nonce==[0;16]{return Err("frame_shape_refused")}Ok(f)
 }
 pub fn expect(self,nonce:[u8;16],tag:Tag,seq:u32,stamp:Stamp)->Result<()>{if self.nonce!=nonce||self.tag!=tag||self.seq!=seq||self.stamp!=stamp{return Err("unexpected_frame")}Self::decode(&self.encode()).map(|_|())}
}
pub const EMPTY:Stamp=Stamp{pid:0,created:0};
#[derive(Clone,Copy,Debug,PartialEq,Eq)]pub enum Phase{Hello,Ready,Register,Ack,Running,Done,Released,Cancelled}
pub struct Launcher{nonce:[u8;16],helper:Stamp,target:Option<Stamp>,pub phase:Phase}
impl Launcher{
 pub fn new(nonce:[u8;16],helper:Stamp)->Result<Self>{if nonce==[0;16]||!helper.valid(){return Err("session_identity_required")}Ok(Self{nonce,helper,target:None,phase:Phase::Hello})}
 pub fn hello(&mut self,f:Frame)->Result<()>{if self.phase!=Phase::Hello{return Err("phase_refused")}f.expect(self.nonce,Tag::Hello,0,self.helper)?;self.phase=Phase::Ready;Ok(())}
 pub fn ready(&mut self,f:Frame)->Result<()>{if self.phase!=Phase::Ready{return Err("phase_refused")}f.expect(self.nonce,Tag::Ready,1,EMPTY)?;self.phase=Phase::Register;Ok(())}
 pub fn register(&mut self,target:Stamp)->Result<Frame>{if self.phase!=Phase::Register||!target.valid(){return Err("registration_refused")}self.target=Some(target);self.phase=Phase::Ack;Ok(Frame::new(Tag::Register,self.nonce,2,target,0))}
 pub fn ack(&mut self,f:Frame)->Result<()>{if self.phase!=Phase::Ack{return Err("phase_refused")}f.expect(self.nonce,Tag::Ack,3,self.target.ok_or("target_required")?)?;self.phase=Phase::Running;Ok(())}
 pub fn resume_once(&mut self)->Result<()>{if self.phase!=Phase::Running{return Err("resume_before_ack_or_repeated")}self.phase=Phase::Done;Ok(())}
 pub fn done(&self,local_cleanup_verified:bool)->Result<Frame>{if self.phase!=Phase::Done||!local_cleanup_verified{return Err("local_cleanup_required")}Ok(Frame::new(Tag::Done,self.nonce,4,self.target.ok_or("target_required")?,0))}
 pub fn released(&mut self,f:Frame)->Result<()>{if self.phase!=Phase::Done{return Err("phase_refused")}f.expect(self.nonce,Tag::Released,5,self.target.ok_or("target_required")?)?;self.phase=Phase::Released;Ok(())}
 pub fn cancel_without_target(&mut self)->Result<Frame>{if self.phase!=Phase::Register||self.target.is_some(){return Err("cancel_refused")}self.phase=Phase::Cancelled;Ok(Frame::new(Tag::Cancel,self.nonce,2,EMPTY,0))}
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]pub enum Recovery{WaitLauncherExit,WaitTargetExit}
pub fn recovery(registered_target:Option<Stamp>)->Recovery{if registered_target.is_some(){Recovery::WaitTargetExit}else{Recovery::WaitLauncherExit}}
pub struct GuardRetention{expected:Recovery,exit_verified:bool}
impl GuardRetention{
 pub fn new(target:Option<Stamp>)->Self{Self{expected:recovery(target),exit_verified:false}}
 pub fn observe(&mut self,subject:Recovery,exited:Option<bool>)->bool{if subject==self.expected&&exited==Some(true){self.exit_verified=true;}self.exit_verified}
}
#[derive(Clone,Copy)]pub struct PeerFacts{pub expected_pid:u32,pub actual_pid:u32,pub expected_created:u64,pub actual_created:u64,pub same_image:bool,pub same_hash:bool,pub same_sid:bool,pub same_session:bool,pub expected_elevated:bool,pub actual_elevated:bool,pub expected_integrity:u32,pub actual_integrity:u32}
pub fn admit_peer(p:PeerFacts)->Result<()>{if p.expected_pid==0||p.expected_created==0||p.expected_pid!=p.actual_pid||p.expected_created!=p.actual_created||!p.same_image||!p.same_hash||!p.same_sid||!p.same_session||p.expected_elevated!=p.actual_elevated||p.expected_integrity!=p.actual_integrity{return Err("peer_identity_refused")}Ok(())}
#[cfg(test)]mod tests{
 use super::*;fn stamp()->Stamp{Stamp{pid:11,created:22}}fn frame(tag:Tag,seq:u32,s:Stamp,status:u32)->Frame{Frame::new(tag,[7;16],seq,s,status)}
 #[test]fn closed_frame_rejects_truncation_extra_fields_and_unknown_tags(){let f=frame(Tag::Register,2,stamp(),0);let b=f.encode();assert_eq!(Frame::decode(&b),Ok(f));for n in [0,1,63,65]{assert!(Frame::decode(&vec![0;n]).is_err());}for i in [0,4,5,6,44,63]{let mut x=b;x[i]=255;assert!(Frame::decode(&x).is_err());}let mut x=b;x[40]=1;assert!(Frame::decode(&x).is_err());}
 #[test]fn resume_requires_authenticated_ready_registration_and_exact_ack_once(){let mut s=Launcher::new([7;16],stamp()).unwrap();assert!(s.resume_once().is_err());assert!(s.register(stamp()).is_err());s.hello(frame(Tag::Hello,0,stamp(),0)).unwrap();assert!(s.resume_once().is_err());s.ready(frame(Tag::Ready,1,EMPTY,4)).unwrap();assert!(s.resume_once().is_err());let target=Stamp{pid:33,created:44};s.register(target).unwrap();assert!(s.resume_once().is_err());assert!(s.ack(frame(Tag::Ack,3,stamp(),0)).is_err());s.ack(frame(Tag::Ack,3,target,0)).unwrap();s.resume_once().unwrap();assert!(s.resume_once().is_err());assert!(s.done(false).is_err());s.done(true).unwrap();s.released(frame(Tag::Released,5,target,0)).unwrap();}
 #[test]fn spoofed_nonce_pid_time_or_ready_never_advances(){for i in 0..5{let mut s=Launcher::new([7;16],stamp()).unwrap();let mut f=frame(Tag::Hello,0,stamp(),0);match i{0=>f.nonce[0]^=1,1=>f.stamp.pid+=1,2=>f.stamp.created+=1,3=>f.tag=Tag::Ready,4=>f.seq=1,_=>{}}assert!(s.hello(f).is_err());assert_eq!(s.phase,Phase::Hello);}let mut s=Launcher::new([7;16],stamp()).unwrap();s.hello(frame(Tag::Hello,0,stamp(),0)).unwrap();assert!(s.ready(frame(Tag::Ready,1,EMPTY,3)).is_err());assert_eq!(s.phase,Phase::Ready);}
 #[test]fn lost_ack_cancel_and_parent_death_require_retained_target_exit(){assert_eq!(recovery(None),Recovery::WaitLauncherExit);assert_eq!(recovery(Some(stamp())),Recovery::WaitTargetExit);let mut s=Launcher::new([7;16],stamp()).unwrap();s.hello(frame(Tag::Hello,0,stamp(),0)).unwrap();s.ready(frame(Tag::Ready,1,EMPTY,4)).unwrap();let mut clean=Launcher::new([7;16],stamp()).unwrap();clean.hello(frame(Tag::Hello,0,stamp(),0)).unwrap();clean.ready(frame(Tag::Ready,1,EMPTY,4)).unwrap();assert_eq!(clean.cancel_without_target().unwrap().tag,Tag::Cancel);s.register(stamp()).unwrap();assert!(s.cancel_without_target().is_err());assert!(s.resume_once().is_err());}
 #[test]fn every_os_identity_fact_is_required(){let p=PeerFacts{expected_pid:1,actual_pid:1,expected_created:2,actual_created:2,same_image:true,same_hash:true,same_sid:true,same_session:true,expected_elevated:true,actual_elevated:true,expected_integrity:12288,actual_integrity:12288};assert_eq!(admit_peer(p),Ok(()));for i in 0..10{let mut x=p;match i{0=>x.actual_pid=2,1=>x.actual_created=3,2=>x.same_image=false,3=>x.same_hash=false,4=>x.same_sid=false,5=>x.same_session=false,6=>x.actual_elevated=false,7=>x.expected_pid=0,8=>x.actual_integrity=8192,9=>x.actual_integrity=4096,_=>{}}assert!(admit_peer(x).is_err());}}

 #[test]fn soft_timeout_query_error_parent_death_and_lost_ack_do_not_release_target_guards(){
  let mut retention=GuardRetention::new(Some(stamp()));let mut releases=0;
  for _soft_deadline in 0..130{for observation in [None,Some(false)]{if retention.observe(Recovery::WaitTargetExit,observation){releases+=1;}}assert!(!retention.observe(Recovery::WaitLauncherExit,Some(true)));}
  assert_eq!(releases,0);assert!(retention.observe(Recovery::WaitTargetExit,Some(true)));
  let mut before_registration=GuardRetention::new(None);assert!(!before_registration.observe(Recovery::WaitTargetExit,Some(true)));assert!(before_registration.observe(Recovery::WaitLauncherExit,Some(true)));
 }
}
