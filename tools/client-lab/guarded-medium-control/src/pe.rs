use crate::Result;
pub fn supported_platform(major:u32,minor:u32,build:u32)->bool{major==10&&minor==0&&build>=14393}
fn u16_at(b:&[u8],p:usize)->Result<u16>{Ok(u16::from_le_bytes(b.get(p..p.checked_add(2).ok_or("pe_bounds")?).ok_or("pe_bounds")?.try_into().unwrap()))}
fn u32_at(b:&[u8],p:usize)->Result<u32>{Ok(u32::from_le_bytes(b.get(p..p.checked_add(4).ok_or("pe_bounds")?).ok_or("pe_bounds")?.try_into().unwrap()))}
pub fn guard_system32_imports(b:&[u8])->Result<()>{
 if b.len()>16*1024*1024||b.get(..2)!=Some(b"MZ"){return Err("guard_pe_required")}
 let pe=u32_at(b,0x3c)? as usize;if b.get(pe..pe.checked_add(4).ok_or("pe_bounds")?)!=Some(b"PE\0\0")||u16_at(b,pe+4)?!=0x8664{return Err("guard_pe64_required")}
 let sections=u16_at(b,pe+6)? as usize;let opt=pe.checked_add(24).ok_or("pe_bounds")?;let size=u16_at(b,pe+20)? as usize;
 if sections==0||sections>96||size<240||u16_at(b,opt)?!=0x20b||u32_at(b,opt+108)?<11{return Err("guard_load_config_required")}
 let rva=u32_at(b,opt+112+10*8)?;let amount=u32_at(b,opt+116+10*8)?;
 if amount<80{return Err("guard_load_config_required")}
 let table=opt.checked_add(size).ok_or("pe_bounds")?;
 let table_end=table.checked_add(sections*40).ok_or("pe_bounds")?;
 b.get(opt..table_end).ok_or("pe_bounds")?;
 let target_end=rva.checked_add(80).ok_or("guard_load_config_bounds")?;
 let mut mappings=Vec::with_capacity(sections);
 let mut selected=None;
 for index in 0..sections{
  let section=table+index*40;
  let virtual_size=u32_at(b,section+8)?;
  let start=u32_at(b,section+12)?;
  let raw=u32_at(b,section+16)?;
  let pointer=u32_at(b,section+20)? as usize;
  let virtual_end=start.checked_add(virtual_size.max(raw)).ok_or("guard_section_bounds")?;
  let raw_end=pointer.checked_add(raw as usize).ok_or("guard_section_bounds")?;
  if raw_end>b.len(){return Err("guard_section_bounds")}
  mappings.push((start,raw,pointer,raw_end));
  if start<target_end&&rva<virtual_end{
   if selected.is_some(){return Err("guard_load_config_ambiguous")}
   selected=Some(index);
  }
 }
 let selected=selected.ok_or("guard_load_config_required")?;
 let(start,raw,pointer,_)=mappings[selected];
 if rva<start||target_end>start.checked_add(raw).ok_or("guard_section_bounds")?{return Err("guard_load_config_bounds")}
 let offset=pointer.checked_add((rva-start)as usize).ok_or("pe_bounds")?;
 let end=offset.checked_add(80).ok_or("pe_bounds")?;
 for(index,(_,raw,other_start,other_end))in mappings.iter().enumerate(){
  if index!=selected&&*raw>0&&*other_start<end&&offset<*other_end{return Err("guard_load_config_ambiguous")}
 }
 if end>b.len()||u32_at(b,offset)?<80{return Err("guard_load_config_bounds")}
 let found=Some(u16_at(b,offset+78)?);
 if found!=Some(0x800){return Err("guard_system32_imports_required")}Ok(())
}
#[cfg(test)]mod tests{
 use super::*;fn fixture()->Vec<u8>{let mut b=vec![0;1024];b[..2].copy_from_slice(b"MZ");b[0x3c..0x40].copy_from_slice(&64u32.to_le_bytes());b[64..68].copy_from_slice(b"PE\0\0");b[68..70].copy_from_slice(&0x8664u16.to_le_bytes());b[70..72].copy_from_slice(&1u16.to_le_bytes());b[84..86].copy_from_slice(&240u16.to_le_bytes());b[88..90].copy_from_slice(&0x20bu16.to_le_bytes());b[196..200].copy_from_slice(&16u32.to_le_bytes());b[280..284].copy_from_slice(&0x1000u32.to_le_bytes());b[284..288].copy_from_slice(&80u32.to_le_bytes());b[340..344].copy_from_slice(&0x1000u32.to_le_bytes());b[344..348].copy_from_slice(&256u32.to_le_bytes());b[348..352].copy_from_slice(&512u32.to_le_bytes());b[512..516].copy_from_slice(&80u32.to_le_bytes());b[590..592].copy_from_slice(&0x800u16.to_le_bytes());b}
 #[test]fn static_search_flag_requires_documented_windows_floor(){assert!(supported_platform(10,0,14393));assert!(supported_platform(10,0,26100));for version in [(10,0,14392),(6,3,9600),(11,0,0),(10,1,99999)]{assert!(!supported_platform(version.0,version.1,version.2));}}
 #[test]fn exact_guard_load_flag_and_bounds_required(){let b=fixture();assert_eq!(guard_system32_imports(&b),Ok(()));for flag in [0u16,0x200,0x1000,0x801]{let mut x=b.clone();x[590..592].copy_from_slice(&flag.to_le_bytes());assert!(guard_system32_imports(&x).is_err());}for n in [0,63,87,328,591]{assert!(guard_system32_imports(&b[..n]).is_err());}}

 #[test]fn partial_virtual_or_raw_aliases_cannot_supply_any_load_config_byte(){
  fn second(mut b:Vec<u8>,virtual_start:u32,virtual_size:u32,raw_size:u32,raw_pointer:u32)->Vec<u8>{b[70..72].copy_from_slice(&2u16.to_le_bytes());for(at,value)in[(376,virtual_size),(380,virtual_start),(384,raw_size),(388,raw_pointer)]{b[at..at+4].copy_from_slice(&value.to_le_bytes());}b}
  for(start,virtual_size,raw,pointer)in[(0x104f,1,1,768),(0x104e,2,2,768),(0x1000,1,1,768),(0x0fff,2,0,0),(0x2000,1,1,590),(0x2000,1,1,591)]{
   assert!(guard_system32_imports(&second(fixture(),start,virtual_size,raw,pointer)).is_err(),"partial alias accepted");
  }
  assert_eq!(guard_system32_imports(&second(fixture(),0x2000,16,16,768)),Ok(()));
 }
}
