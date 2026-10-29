//! Windows API adapter. Called only from the explicit native fixture role.
use metallic_external_guard_fixture::{Api, Budget, Layer, Result, Rule, REQUESTED_FILTER_FLAGS};
use std::{ffi::c_void, ptr, slice};
use windows_sys::{core::GUID, Win32::{Foundation::HANDLE, NetworkManagement::WindowsFilteringPlatform::*, System::Rpc::{UuidCreate, RPC_C_AUTHN_WINNT}}};
const _:()=assert!(REQUESTED_FILTER_FLAGS==FWPM_FILTER_FLAG_INDEXED);

fn check(code: u32) -> Result<()> { if code == 0 { Ok(()) } else { Err("wfp_api_failed") } }
fn bytes(g: GUID) -> [u8;16] {
    let mut b=[0;16];b[..4].copy_from_slice(&g.data1.to_le_bytes());b[4..6].copy_from_slice(&g.data2.to_le_bytes());b[6..8].copy_from_slice(&g.data3.to_le_bytes());b[8..].copy_from_slice(&g.data4);b
}
fn unique() -> Result<GUID> { let mut g=GUID::from_u128(0);let code=unsafe{UuidCreate(&mut g)};if code==0||code==1824{Ok(g)}else{Err("guid_failed")} }
fn layer(l:Layer)->GUID { match l { Layer::Connect4=>FWPM_LAYER_ALE_AUTH_CONNECT_V4,Layer::Connect6=>FWPM_LAYER_ALE_AUTH_CONNECT_V6,Layer::Receive4=>FWPM_LAYER_ALE_AUTH_RECV_ACCEPT_V4,Layer::Receive6=>FWPM_LAYER_ALE_AUTH_RECV_ACCEPT_V6 } }
fn from_layer(g:GUID)->Result<Layer> { for l in metallic_external_guard_fixture::LAYERS{if bytes(g)==bytes(layer(l)){return Ok(l)}}Err("readback_layer_unknown") }

pub struct Native { engine:HANDLE, sublayer:GUID, app:Vec<u8>, target:std::path::PathBuf, budget:Budget }
impl Native {
    pub fn new(budget:Budget,target:std::path::PathBuf) -> Result<Self> { budget.check()?;let result=Self{engine:ptr::null_mut(),sublayer:unique()?,app:Vec::new(),target,budget};result.budget.check()?;Ok(result) }

}
impl Api for Native {
    fn open_dynamic(&mut self)->Result<()> {
        self.budget.check()?;
        if !self.engine.is_null(){return Err("session_already_open")}
        let name:Vec<u16>=self.target.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut app=ptr::null_mut();
        check(unsafe{FwpmGetAppIdFromFileName0(name.as_ptr(),&mut app)})?;
        let copied=(||{if app.is_null(){return Err("empty_app_id")};let blob=unsafe{&*app};if blob.size==0||blob.size>32768||blob.data.is_null(){return Err("app_id_bound")};Ok(unsafe{slice::from_raw_parts(blob.data,blob.size as usize)}.to_vec())})();
        unsafe{FwpmFreeMemory0(&mut app as *mut _ as *mut *mut c_void)};
        self.app=copied?;
        self.budget.check()?;
        let session=FWPM_SESSION0{sessionKey:unique()?,flags:FWPM_SESSION_FLAG_DYNAMIC,txnWaitTimeoutInMSec:1000,..Default::default()};
        check(unsafe{FwpmEngineOpen0(ptr::null(),RPC_C_AUTHN_WINNT,ptr::null(),&session,&mut self.engine)})?;self.budget.check()
    }
    fn begin(&mut self)->Result<()>{self.budget.check()?;check(unsafe{FwpmTransactionBegin0(self.engine,0)})?;self.budget.check()}
    fn add_sublayer(&mut self)->Result<()> {
        self.budget.check()?;
        let mut title:Vec<u16>="Metallic OWNED fixture only".encode_utf16().chain(Some(0)).collect();
        let sub=FWPM_SUBLAYER0{subLayerKey:self.sublayer,displayData:FWPM_DISPLAY_DATA0{name:title.as_mut_ptr(),description:ptr::null_mut()},weight:u16::MAX,..Default::default()};
        check(unsafe{FwpmSubLayerAdd0(self.engine,&sub,ptr::null_mut())})?;self.budget.check()
    }
    fn add(&mut self,l:Layer)->Result<Rule> {
        self.budget.check()?;
        let key=unique()?;
        let mut blob=FWP_BYTE_BLOB{size:self.app.len() as u32,data:self.app.as_mut_ptr()};
        let mut cond=FWPM_FILTER_CONDITION0{fieldKey:FWPM_CONDITION_ALE_APP_ID,matchType:FWP_MATCH_EQUAL,conditionValue:FWP_CONDITION_VALUE0{r#type:FWP_BYTE_BLOB_TYPE,Anonymous:FWP_CONDITION_VALUE0_0{byteBlob:&mut blob}}};
        let mut title:Vec<u16>="Metallic OWNED fixture BLOCK".encode_utf16().chain(Some(0)).collect();
        let f=FWPM_FILTER0{filterKey:key,flags:REQUESTED_FILTER_FLAGS,displayData:FWPM_DISPLAY_DATA0{name:title.as_mut_ptr(),description:ptr::null_mut()},layerKey:layer(l),subLayerKey:self.sublayer,numFilterConditions:1,filterCondition:&mut cond,action:FWPM_ACTION0{r#type:FWP_ACTION_BLOCK,..Default::default()},..Default::default()};
        let mut id=0;check(unsafe{FwpmFilterAdd0(self.engine,&f,ptr::null_mut(),&mut id)})?;
        self.budget.check()?;
        Ok(Rule{id,key:bytes(key),sublayer:bytes(self.sublayer),layer:l,app_id:self.app.clone(),block:true,flags:REQUESTED_FILTER_FLAGS,conditions:1,app_equal:true})
    }
    fn commit(&mut self)->Result<()>{self.budget.check()?;check(unsafe{FwpmTransactionCommit0(self.engine)})?;self.budget.check()}
    fn read(&mut self,id:u64)->Result<Option<Rule>> {
        self.budget.check()?;
        let mut p=ptr::null_mut();let code=unsafe{FwpmFilterGetById0(self.engine,id,&mut p)};
        if code==0x80320003{return Ok(None)}
        check(code)?;
        let result=(||{
            if p.is_null(){return Err("empty_readback")};let f=unsafe{&*p};
            if f.numFilterConditions!=1{return Err("readback_condition_count_mismatch")}
            if f.filterCondition.is_null(){return Err("readback_condition_pointer_missing")}
            let c=unsafe{&*f.filterCondition};
            if bytes(c.fieldKey)!=bytes(FWPM_CONDITION_ALE_APP_ID){return Err("readback_condition_key_mismatch")}
            if c.conditionValue.r#type!=FWP_BYTE_BLOB_TYPE{return Err("readback_condition_value_type_mismatch")}
            let b=unsafe{c.conditionValue.Anonymous.byteBlob};
            if b.is_null(){return Err("readback_app_id_blob_missing")};let b=unsafe{&*b};
            if b.size==0||b.size>32768||b.data.is_null(){return Err("readback_app_id_blob_bound")}
            Ok(Some(Rule{id:f.filterId,key:bytes(f.filterKey),sublayer:bytes(f.subLayerKey),layer:from_layer(f.layerKey)?,app_id:unsafe{slice::from_raw_parts(b.data,b.size as usize)}.to_vec(),block:f.action.r#type==FWP_ACTION_BLOCK,flags:f.flags,conditions:f.numFilterConditions,app_equal:c.matchType==FWP_MATCH_EQUAL}))
        })();
        unsafe{FwpmFreeMemory0(&mut p as *mut _ as *mut *mut c_void)};self.budget.check()?;result
    }
    fn abort(&mut self)->Result<()>{check(unsafe{FwpmTransactionAbort0(self.engine)})}
    fn delete(&mut self,id:u64)->Result<()>{check(unsafe{FwpmFilterDeleteById0(self.engine,id)})}
    fn delete_sublayer(&mut self)->Result<()>{check(unsafe{FwpmSubLayerDeleteByKey0(self.engine,&self.sublayer)})}
    fn close(&mut self)->Result<()> {if self.engine.is_null(){return Ok(())};check(unsafe{FwpmEngineClose0(self.engine)})?;self.engine=ptr::null_mut();Ok(())}
}
impl Drop for Native { fn drop(&mut self){if !self.engine.is_null(){let _=self.close();}} }
use std::os::windows::ffi::OsStrExt;
