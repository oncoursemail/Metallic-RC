use metallic_external_guard_fixture::{role, Role};
#[cfg(windows)] mod wfp;
#[cfg(windows)] mod fixture;
#[cfg(windows)] fn native_fixture(){
    use std::io::{Seek,SeekFrom,Write};
    // Independent of native API progress/GIL-style cooperative scheduling.
    // A hard failure leaves a non-success/partial report, never a success claim.
    std::thread::spawn(||{
        std::thread::sleep(std::time::Duration::from_secs(130));
        std::process::exit(124);
    });
    let report=std::env::current_exe().ok().and_then(|p|p.parent().map(|p|p.join("fixture-report.json")));
    let mut file=match report.and_then(|p|std::fs::OpenOptions::new().write(true).create_new(true).open(p).ok()){
        Some(f)=>f,None=>{eprintln!("{{\"ok\":false,\"error\":\"new_report_required\"}}");std::process::exit(1)}
    };
    if file.write_all(b"{\"ok\":false,\"status\":\"fixture_pending\",\"cleanupVerified\":false}").and_then(|_|file.sync_all()).is_err(){std::process::exit(1)}
    let outcome=fixture::run();
    let (summary,code)=match outcome{
        Ok(())=>("{\"ok\":true,\"scope\":\"owned_fixture_only\",\"beforePositive\":4,\"blocked\":4,\"afterPositive\":4,\"cleanupVerified\":true,\"originalClientTested\":false,\"executionReady\":false,\"fullSandboxClaimed\":false}".to_owned(),0),
        Err(error)=>(format!("{{\"ok\":false,\"error\":\"{error}\",\"cleanupVerified\":false,\"originalClientTested\":false,\"executionReady\":false}}"),1)
    };
    if summary.len()>2048||file.seek(SeekFrom::Start(0)).and_then(|_|file.write_all(summary.as_bytes())).and_then(|_|file.set_len(summary.len() as u64)).and_then(|_|file.sync_all()).is_err(){std::process::exit(1)}
    println!("{summary}");std::process::exit(code)
}
fn main(){
    let args:Vec<String>=std::env::args().skip(1).collect();
    let selected=match role(&args){Ok(r)=>r,Err(_)=>{eprintln!("{{\"ok\":false,\"error\":\"arguments_refused\"}}");std::process::exit(1)}};
    match selected{
        Role::Plan=>println!("{{\"mode\":\"plan\",\"executionReady\":false,\"approvalGranted\":false,\"filtersInstalled\":false,\"target\":\"current_authored_executable_only\",\"filters\":4,\"protocols\":[\"tcp4\",\"tcp6\",\"udp4\",\"udp6\"],\"probeJobStopSeconds\":90,\"hardFailureSeconds\":130,\"fullSandboxClaimed\":false}}"),
        #[cfg(windows)] Role::Probe(p,port)=>std::process::exit(fixture::child(p,port)),
        #[cfg(windows)] Role::Fixture=>native_fixture(),
        #[cfg(not(windows))] _=>{eprintln!("Windows only");std::process::exit(1)}
    }
}
