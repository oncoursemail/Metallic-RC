#![cfg_attr(windows, windows_subsystem = "windows")]
use std::{io::Write,time::Duration};
use metallic_guarded_medium_control::ENVIRONMENT;
fn main(){
    std::thread::spawn(||{std::thread::sleep(Duration::from_secs(120));std::process::exit(124)});
    let args:Vec<String>=std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice(){
        ["--marker"]=>println!("OWNED_MARKER"),
        ["--network-probe",name,port]=>match metallic_guarded_medium_control::network_probe::parse(name,port){Ok((probe,port))=>std::process::exit(metallic_guarded_medium_control::network_probe::child(probe,port)),Err(_)=>std::process::exit(2)},
        ["--environment"]=>{let rows:Vec<_>=std::env::vars().collect();let unknown=rows.iter().filter(|(key,_)|!ENVIRONMENT.iter().any(|name|name.eq_ignore_ascii_case(key))).count();let allowed=rows.len()-unknown;let drive=rows.iter().any(|(key,_)|key.eq_ignore_ascii_case("SystemDrive"));println!("ENV_ALLOWED={allowed};UNEXPECTED={unknown};SYSTEMDRIVE={}",drive as u8);},
        ["--flood"]=>{let(mut out,mut err)=(std::io::stdout(),std::io::stderr());for _ in 0..512{out.write_all(&[b'A';4096]).unwrap();err.write_all(&[b'B';4096]).unwrap();}},
        ["--wait"]=>{println!("OWNED_WAITING");std::thread::sleep(Duration::from_secs(60));},
        ["--grandchild"]=>{let mut command=std::process::Command::new(std::env::current_exe().unwrap());command.arg("--wait");#[cfg(windows)]{use std::os::windows::process::CommandExt;command.creation_flags(0x08000000);}let mut child=command.spawn().unwrap();let _=child.wait();},
        _=>std::process::exit(2),
    }
}
