//! Scoped diagnostic correlation only. No result here changes probe acceptance.
use crate::{Layer, Probe};

pub const FRAME_BYTES:usize=32;
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
#[repr(u8)]
pub enum Operation { Bind=0, ReadTimeout=1, WriteTimeout=2, Connect=3, LocalAddress=4, Send=5, Receive=6, Reply=7, Complete=8 }
impl Operation {
    pub fn name(self)->&'static str{match self{Self::Bind=>"bind",Self::ReadTimeout=>"read_timeout",Self::WriteTimeout=>"write_timeout",Self::Connect=>"connect",Self::LocalAddress=>"local_address",Self::Send=>"send",Self::Receive=>"receive",Self::Reply=>"reply",Self::Complete=>"complete"}}
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct Record { pub family:u8,pub operation:Operation,pub source_port:u16,pub destination_port:u16,pub sent:u8 }
impl Record {
    pub fn new(probe:Probe,port:u16)->Option<Self>{let family=match probe{Probe::Udp4=>4,Probe::Udp6=>6,_=>return None};Some(Self{family,operation:Operation::Bind,source_port:0,destination_port:port,sent:0})}
    pub fn encode(self)->[u8;FRAME_BYTES]{let mut b=[0;FRAME_BYTES];b[..4].copy_from_slice(b"MRU1");b[4]=self.family;b[5]=self.operation as u8;b[6]=self.sent;b[8..10].copy_from_slice(&self.source_port.to_le_bytes());b[10..12].copy_from_slice(&self.destination_port.to_le_bytes());b}
    pub fn decode(b:&[u8],probe:Probe,port:u16)->Option<Self>{
        let expected=Self::new(probe,port)?;
        if b.len()!=FRAME_BYTES||&b[..4]!=b"MRU1"||b[4]!=expected.family||b[7]!=0||b[12..].iter().any(|v|*v!=0)||b[6]>4{return None}
        let operation=match b[5]{0=>Operation::Bind,1=>Operation::ReadTimeout,2=>Operation::WriteTimeout,3=>Operation::Connect,4=>Operation::LocalAddress,5=>Operation::Send,6=>Operation::Receive,7=>Operation::Reply,8=>Operation::Complete,_=>return None};
        let source_port=u16::from_le_bytes(b[8..10].try_into().ok()?);let destination_port=u16::from_le_bytes(b[10..12].try_into().ok()?);
        if source_port==0||destination_port==0||destination_port!=port||(b[6]>0&&(operation as u8)<Operation::Send as u8){return None}
        Some(Self{family:expected.family,operation,source_port,destination_port,sent:b[6]})
    }
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum Status { Unavailable, Disabled, Inconclusive, Matched, Capped, Deadline }
impl Status{pub fn name(self)->&'static str{match self{Self::Unavailable=>"unavailable",Self::Disabled=>"disabled",Self::Inconclusive=>"inconclusive",Self::Matched=>"matched",Self::Capped=>"capped",Self::Deadline=>"deadline"}}}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum ApiStage { CollectionGet, LayerGet, EnumCreate, EnumPage, EnumDestroy }
impl ApiStage{pub fn name(self)->&'static str{match self{Self::CollectionGet=>"collection_get",Self::LayerGet=>"layer_get",Self::EnumCreate=>"enum_create",Self::EnumPage=>"enum_page",Self::EnumDestroy=>"enum_destroy"}}}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct ApiFailure { pub stage:ApiStage,pub code:u32 }
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct Stats { pub status:Status,pub outbound:u16,pub inbound:u16,pub api:Option<ApiFailure> }
impl Stats{pub fn empty(status:Status)->Self{Self{status,outbound:0,inbound:0,api:None}}}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct Observation { pub record:Option<Record>,pub start:u64,pub end:u64,pub stats:Stats }
impl Default for Observation{fn default()->Self{Self{record:None,start:0,end:0,stats:Stats::empty(Status::Unavailable)}}}
#[derive(Clone,Copy)]
pub struct Scope{pub record:Record,pub start:u64,pub end:u64,pub filter_id:u64,pub layer_id:u16,pub outbound:bool}
impl Scope{
    pub fn valid(&self)->bool{self.start>0&&self.end>=self.start&&self.end-self.start<=100_000_000&&self.record.source_port>0&&self.record.destination_port>0&&self.record.sent==4&&(self.record.operation as u8)>=Operation::Receive as u8&&self.filter_id>0&&self.layer_id>0&&matches!(self.record.family,4|6)}
    pub fn ports(&self)->(u16,u16){if self.outbound{(self.record.source_port,self.record.destination_port)}else{(self.record.destination_port,self.record.source_port)}}
}
#[derive(Clone,Copy)]
pub struct Event{pub drop:bool,pub presence:u32,pub family:u8,pub protocol:u8,pub local_loopback:bool,pub remote_loopback:bool,pub local_port:u16,pub remote_port:u16,pub app_equal:bool,pub timestamp:u64,pub filter_id:u64,pub layer_id:u16,pub direction:u32,pub is_loopback:bool}
pub fn matches(event:Event,scope:Scope)->bool{
    let(local,remote)=scope.ports();scope.valid()&&event.drop&&event.presence&0x13f==0x13f&&event.family==scope.record.family&&event.protocol==17&&event.local_loopback&&event.remote_loopback&&event.local_port==local&&event.remote_port==remote&&event.app_equal&&event.timestamp>=scope.start&&event.timestamp<=scope.end&&event.filter_id==scope.filter_id&&event.layer_id==scope.layer_id&&event.direction==if scope.outbound{0x3901}else{0x3900}&&event.is_loopback
}
pub fn wanted_layer(family:u8,outbound:bool)->Option<Layer>{match(family,outbound){(4,true)=>Some(Layer::Connect4),(6,true)=>Some(Layer::Connect6),(4,false)=>Some(Layer::Receive4),(6,false)=>Some(Layer::Receive6),_=>None}}

#[cfg(windows)]
mod native{
    use super::*;
    use crate::{Budget,Rule};
    use std::{ffi::c_void,ptr,slice,time::{Duration,Instant}};
    use windows_sys::{core::GUID,Win32::{Foundation::{FILETIME,HANDLE},NetworkManagement::WindowsFilteringPlatform::*}};
    const MAX_BYTES:usize=65536;
    fn guid(l:Layer)->GUID{match l{Layer::Connect4=>FWPM_LAYER_ALE_AUTH_CONNECT_V4,Layer::Connect6=>FWPM_LAYER_ALE_AUTH_CONNECT_V6,Layer::Receive4=>FWPM_LAYER_ALE_AUTH_RECV_ACCEPT_V4,Layer::Receive6=>FWPM_LAYER_ALE_AUTH_RECV_ACCEPT_V6}}
    fn same(a:GUID,b:GUID)->bool{a.data1==b.data1&&a.data2==b.data2&&a.data3==b.data3&&a.data4==b.data4}
    struct Memory(*mut c_void);
    impl Drop for Memory{fn drop(&mut self){if !self.0.is_null(){unsafe{FwpmFreeMemory0(&mut self.0)}}}}
    struct Enumeration{engine:HANDLE,handle:HANDLE}
    impl Enumeration{fn close(&mut self)->u32{if self.handle.is_null(){return 0}let code=unsafe{FwpmNetEventDestroyEnumHandle0(self.engine,self.handle)};if code==0{self.handle=ptr::null_mut();}code}}
    impl Drop for Enumeration{fn drop(&mut self){let _=self.close();}}
    fn filetime(t:u64)->FILETIME{FILETIME{dwLowDateTime:t as u32,dwHighDateTime:(t>>32)as u32}}
    fn allowed(start:Instant,budget:&Budget)->bool{start.elapsed()<Duration::from_secs(2)&&budget.check().is_ok()}
    fn api_failure(stage:ApiStage,code:u32,start:Instant,budget:&Budget)->Stats{Stats{api:Some(ApiFailure{stage,code}),..Stats::empty(if allowed(start,budget){Status::Unavailable}else{Status::Deadline})}}
    fn condition(key:GUID,kind:FWP_DATA_TYPE,value:FWP_CONDITION_VALUE0_0)->FWPM_FILTER_CONDITION0{FWPM_FILTER_CONDITION0{fieldKey:key,matchType:FWP_MATCH_EQUAL,conditionValue:FWP_CONDITION_VALUE0{r#type:kind,Anonymous:value}}}
    fn layer_id(engine:HANDLE,layer:Layer,start:Instant,budget:&Budget)->std::result::Result<u16,Stats>{
        if !allowed(start,budget){return Err(Stats::empty(Status::Deadline))}
        let key=guid(layer);let mut p=ptr::null_mut();let code=unsafe{FwpmLayerGetByKey0(engine,&key,&mut p)};let memory=Memory(p as *mut c_void);
        if code!=0{return Err(api_failure(ApiStage::LayerGet,code,start,budget))}
        if !allowed(start,budget){return Err(Stats::empty(Status::Deadline))}
        if p.is_null(){return Err(Stats::empty(Status::Unavailable))}let value=unsafe{&*p};if !same(value.layerKey,key)||value.layerId==0{return Err(Stats::empty(Status::Unavailable))}let id=value.layerId;
        drop(memory);if !allowed(start,budget){return Err(Stats::empty(Status::Deadline))}Ok(id)
    }
    fn query(engine:HANDLE,app:&[u8],scope:Scope,start:Instant,budget:&Budget,bytes:&mut usize)->std::result::Result<u16,Stats>{
        if !scope.valid()||!allowed(start,budget){return Err(Stats::empty(Status::Deadline))}
        let(mut app_bytes,mut loop6)=(app.to_vec(),FWP_BYTE_ARRAY16{byteArray16:[0;16]});loop6.byteArray16[15]=1;
        let mut blob=FWP_BYTE_BLOB{size:app_bytes.len()as u32,data:app_bytes.as_mut_ptr()};
        let(local,remote)=scope.ports();
        // One documented logical encoding only: IPv4 host scalar, IPv6 octets,
        // host-order ports. No alternate-endian matching fallback is attempted.
        let(ip_kind,ip_value)=if scope.record.family==4{(FWP_UINT32,FWP_CONDITION_VALUE0_0{uint32:0x7f000001})}else{(FWP_BYTE_ARRAY16_TYPE,FWP_CONDITION_VALUE0_0{byteArray16:&mut loop6})};
        let mut conditions=[
            condition(FWPM_CONDITION_ALE_APP_ID,FWP_BYTE_BLOB_TYPE,FWP_CONDITION_VALUE0_0{byteBlob:&mut blob}),
            condition(FWPM_CONDITION_IP_PROTOCOL,FWP_UINT8,FWP_CONDITION_VALUE0_0{uint8:17}),
            condition(FWPM_CONDITION_IP_LOCAL_ADDRESS,ip_kind,ip_value),condition(FWPM_CONDITION_IP_REMOTE_ADDRESS,ip_kind,ip_value),
            condition(FWPM_CONDITION_IP_LOCAL_PORT,FWP_UINT16,FWP_CONDITION_VALUE0_0{uint16:local}),condition(FWPM_CONDITION_IP_REMOTE_PORT,FWP_UINT16,FWP_CONDITION_VALUE0_0{uint16:remote}),
            condition(FWPM_CONDITION_NET_EVENT_TYPE,FWP_UINT32,FWP_CONDITION_VALUE0_0{uint32:FWPM_NET_EVENT_TYPE_CLASSIFY_DROP as u32}),
        ];
        let template=FWPM_NET_EVENT_ENUM_TEMPLATE0{startTime:filetime(scope.start),endTime:filetime(scope.end),numFilterConditions:conditions.len()as u32,filterCondition:conditions.as_mut_ptr()};
        let mut handle=ptr::null_mut();let code=unsafe{FwpmNetEventCreateEnumHandle0(engine,&template,&mut handle)};
        let mut enumeration=Enumeration{engine,handle};
        if code!=0{return Err(api_failure(ApiStage::EnumCreate,code,start,budget))}
        if !allowed(start,budget){return Err(Stats::empty(Status::Deadline))}
        if handle.is_null(){return Err(Stats::empty(Status::Unavailable))}
        let result=(||{
            let mut count=0u16;
            for _ in 0..2{
                if !allowed(start,budget){return Err(Stats::empty(Status::Deadline))}
                let mut entries=ptr::null_mut();let mut returned=0;
                let code=unsafe{FwpmNetEventEnum2(engine,enumeration.handle,8,&mut entries,&mut returned)};
                let _memory=Memory(entries as *mut c_void);
                if code!=0{return Err(api_failure(ApiStage::EnumPage,code,start,budget))}
                if !allowed(start,budget){return Err(Stats::empty(Status::Deadline))}
                if returned>8||(returned>0&&entries.is_null()){return Err(Stats::empty(Status::Unavailable))}
                for i in 0..returned as usize{
                    if !allowed(start,budget){return Err(Stats::empty(Status::Deadline))}
                    *bytes=bytes.checked_add(std::mem::size_of::<FWPM_NET_EVENT2>()+std::mem::size_of::<FWPM_NET_EVENT_CLASSIFY_DROP2>()).ok_or(Stats::empty(Status::Capped))?;
                    if *bytes>MAX_BYTES{return Err(Stats::empty(Status::Capped))}
                    let p=unsafe{*entries.add(i)};if p.is_null(){return Err(Stats::empty(Status::Unavailable))}let e=unsafe{&*p};
                    if e.r#type!=FWPM_NET_EVENT_TYPE_CLASSIFY_DROP||e.header.flags&0x13f!=0x13f{continue}
                    let h=&e.header;
                    let timestamp=((h.timeStamp.dwHighDateTime as u64)<<32)|h.timeStamp.dwLowDateTime as u64;
                    if h.ipProtocol!=17||h.localPort!=local||h.remotePort!=remote||timestamp<scope.start||timestamp>scope.end{continue}
                    if h.appId.size as usize!=app.len()||h.appId.data.is_null(){continue}
                    *bytes=bytes.checked_add(app.len()).ok_or(Stats::empty(Status::Capped))?;if *bytes>MAX_BYTES{return Err(Stats::empty(Status::Capped))}
                    let app_equal=unsafe{slice::from_raw_parts(h.appId.data,app.len())}==app;
                    if !app_equal{continue}
                    let drop=unsafe{e.Anonymous.classifyDrop};if drop.is_null(){continue}let drop=unsafe{&*drop};
                    if drop.filterId!=scope.filter_id||drop.layerId!=scope.layer_id||drop.msFwpDirection!=if scope.outbound{0x3901}else{0x3900}||drop.isLoopback!=1{continue}
                    let(family,local_loopback,remote_loopback)=match h.ipVersion{
                        FWP_IP_VERSION_V4=>(4,unsafe{h.Anonymous1.localAddrV4}==0x7f000001,unsafe{h.Anonymous2.remoteAddrV4}==0x7f000001),
                        FWP_IP_VERSION_V6=>(6,unsafe{h.Anonymous1.localAddrV6.byteArray16}==loop6.byteArray16,unsafe{h.Anonymous2.remoteAddrV6.byteArray16}==loop6.byteArray16),
                        _=>continue,
                    };
                    let event=Event{drop:true,presence:h.flags,family,protocol:h.ipProtocol,local_loopback,remote_loopback,local_port:h.localPort,remote_port:h.remotePort,app_equal,timestamp,filter_id:drop.filterId,layer_id:drop.layerId,direction:drop.msFwpDirection,is_loopback:drop.isLoopback==1};
                    if matches(event,scope){count+=1;}
                }
                if returned<8{return Ok(count)}
            }
            Err(Stats::empty(Status::Capped))
        })();
        let code=enumeration.close();if code!=0{return Err(api_failure(ApiStage::EnumDestroy,code,start,budget))}if !allowed(start,budget){return Err(Stats::empty(Status::Deadline))}result
    }
    pub fn observe(engine:usize,app:&[u8],rules:&[Rule],record:Record,begin:u64,end:u64,budget:&Budget)->Stats{
        let start=Instant::now();let engine=engine as HANDLE;
        if engine.is_null()||app.is_empty()||app.len()>32768||rules.len()!=4||!(Scope{record,start:begin,end,filter_id:1,layer_id:1,outbound:true}).valid(){return Stats::empty(Status::Unavailable)}
        let mut value=ptr::null_mut();let code=unsafe{FwpmEngineGetOption0(engine,FWPM_ENGINE_COLLECT_NET_EVENTS,&mut value)};
        let memory=Memory(value as *mut c_void);
        if code!=0{return api_failure(ApiStage::CollectionGet,code,start,budget)}
        if !allowed(start,budget){return Stats::empty(Status::Deadline)}
        if value.is_null(){return Stats::empty(Status::Unavailable)}
        let value=unsafe{&*value};if value.r#type!=FWP_UINT32{return Stats::empty(Status::Unavailable)}
        let collection=unsafe{value.Anonymous.uint32};drop(memory);if !allowed(start,budget){return Stats::empty(Status::Deadline)}
        match collection{0=>return Stats::empty(Status::Disabled),1=>{},_=>return Stats::empty(Status::Unavailable)}
        let mut stats=Stats::empty(Status::Inconclusive);let mut bytes=0;
        for outbound in [true,false]{
            if !allowed(start,budget){return Stats::empty(Status::Deadline)}
            let layer=match wanted_layer(record.family,outbound){Some(v)=>v,None=>return Stats::empty(Status::Unavailable)};
            let matching:Vec<_>=rules.iter().filter(|r|r.layer==layer&&r.app_id==app).collect();if matching.len()!=1{return Stats::empty(Status::Unavailable)}
            let layer_id=match layer_id(engine,layer,start,budget){Ok(id)=>id,Err(stats)=>return stats};
            let scope=Scope{record,start:begin,end,filter_id:matching[0].id,layer_id,outbound};if !scope.valid(){return Stats::empty(Status::Unavailable)}
            match query(engine,app,scope,start,budget,&mut bytes){Ok(n)=>if outbound{stats.outbound=n}else{stats.inbound=n},Err(stats)=>return stats}
        }
        if !allowed(start,budget){return Stats::empty(Status::Deadline)}if stats.outbound>0||stats.inbound>0{stats.status=Status::Matched;}stats
    }
    #[cfg(test)] mod tests{
        use super::*;
        #[test]fn api_error_keeps_actual_u32_and_deadline_without_observations(){
            let budget=Budget::default();
            for stage in [ApiStage::CollectionGet,ApiStage::LayerGet,ApiStage::EnumCreate,ApiStage::EnumPage,ApiStage::EnumDestroy]{for code in [5,50,0x8032000d,u32::MAX]{
                let failure=api_failure(stage,code,Instant::now(),&budget);assert_eq!(failure.api,Some(ApiFailure{stage,code}));assert_eq!(failure.status,Status::Unavailable);assert_eq!((failure.outbound,failure.inbound),(0,0));
            }}
            budget.expire();let late=api_failure(ApiStage::EnumPage,5,Instant::now(),&budget);assert_eq!(late.status,Status::Deadline);assert_eq!(late.api,Some(ApiFailure{stage:ApiStage::EnumPage,code:5}));
        }
    }
}
#[cfg(windows)] pub use native::observe;

#[cfg(test)] mod tests{
    use super::*;
    fn record()->Record{Record{family:4,operation:Operation::Receive,source_port:42123,destination_port:52123,sent:4}}
    fn scope()->Scope{Scope{record:record(),start:100,end:200,filter_id:99,layer_id:48,outbound:true}}
    fn event()->Event{Event{drop:true,presence:0x13f,family:4,protocol:17,local_loopback:true,remote_loopback:true,local_port:42123,remote_port:52123,app_equal:true,timestamp:150,filter_id:99,layer_id:48,direction:0x3901,is_loopback:true}}
    #[test]fn fixed_metadata_roundtrip_and_invalid_frames(){let b=record().encode();assert_eq!(Record::decode(&b,Probe::Udp4,52123),Some(record()));for n in [0,1,31,33,64]{assert_eq!(Record::decode(&vec![0;n],Probe::Udp4,52123),None);}for i in [0,1,2,3,4,5,6,7,10,12,31]{let mut bad=b;bad[i]=255;assert_eq!(Record::decode(&bad,Probe::Udp4,52123),None);}let mut bad=b;bad[8]=0;bad[9]=0;assert_eq!(Record::decode(&bad,Probe::Udp4,52123),None);assert_eq!(Record::decode(&b,Probe::Udp6,52123),None);assert_eq!(Record::decode(&b,Probe::Tcp4,52123),None);}
    #[test]fn exact_match_and_every_required_event_field(){assert!(matches(event(),scope()));for i in 0..15{let mut e=event();match i{0=>e.drop=false,1=>e.presence=0,2=>e.family=6,3=>e.protocol=6,4=>e.local_loopback=false,5=>e.remote_loopback=false,6=>e.local_port+=1,7=>e.remote_port+=1,8=>e.app_equal=false,9=>e.timestamp=99,10=>e.timestamp=201,11=>e.filter_id+=1,12=>e.layer_id+=1,13=>e.direction=0,14=>e.is_loopback=false,_=>{}}assert!(!matches(e,scope()));}}
    #[test]fn orientation_and_inbound_are_not_outbound_proof(){let mut s=scope();s.outbound=false;let mut e=event();assert!(!matches(e,s));e.local_port=52123;e.remote_port=42123;e.direction=0x3900;assert!(matches(e,s));assert!(!matches(e,scope()));}
    #[test]fn missing_send_tuple_and_clock_bounds_are_unavailable(){for i in 0..7{let mut s=scope();match i{0=>s.record.sent=0,1=>s.record.source_port=0,2=>s.record.operation=Operation::Send,3=>s.end=99,4=>s.end=100_000_101,5=>s.filter_id=0,6=>s.layer_id=0,_=>{}}assert!(!s.valid());assert!(!matches(event(),s));}}
    #[test]fn required_presence_bits_and_single_header_encoding(){
        for bit in [1,2,4,8,16,32,256]{let mut e=event();e.presence&=!bit;assert!(!matches(e,scope()));}
        let mut e=event();e.local_port=e.local_port.swap_bytes();assert!(!matches(e,scope()));
        let mut e=event();e.remote_port=e.remote_port.swap_bytes();assert!(!matches(e,scope()));
        let mut e=event();e.direction=1;assert!(!matches(e,scope()));
        let mut s=scope();s.record.family=6;let mut e=event();e.family=6;assert!(matches(e,s));
        assert_eq!(wanted_layer(6,true),Some(Layer::Connect6));assert_eq!(wanted_layer(6,false),Some(Layer::Receive6));assert_eq!(wanted_layer(5,true),None);
    }
}
