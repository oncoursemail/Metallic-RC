// Data-only inspection of our release artifact; never executes the guard.
#[cfg(windows)]#[test]fn actual_guard_has_exact_system32_static_import_flag(){
 let path=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/metallic-medium-guard.exe");let bytes=std::fs::read(path).expect("build the reviewed release binaries before this check");
 assert_eq!(metallic_guarded_medium_control::pe::guard_system32_imports(&bytes),Ok(()));
 fn d(b:&[u8],at:usize)->usize{u32::from_le_bytes(b[at..at+4].try_into().unwrap())as usize}
 fn w(b:&[u8],at:usize)->usize{u16::from_le_bytes(b[at..at+2].try_into().unwrap())as usize}
 let pe=d(&bytes,0x3c);let opt=pe+24;let rva=d(&bytes,opt+112+10*8);let sections=opt+w(&bytes,pe+20);let mut offset=None;
 for i in 0..w(&bytes,pe+6){let s=sections+i*40;let at=d(&bytes,s+12);let size=d(&bytes,s+16);if rva>=at&&rva-at<size{offset=Some(d(&bytes,s+20)+rva-at+78);}}
 let at=offset.expect("file-backed load config");assert_eq!(&bytes[at..at+2],&0x800u16.to_le_bytes());
 for flag in [0u16,0x200,0x801,0x1000]{let mut changed=bytes.clone();changed[at..at+2].copy_from_slice(&flag.to_le_bytes());assert!(metallic_guarded_medium_control::pe::guard_system32_imports(&changed).is_err());}
}
