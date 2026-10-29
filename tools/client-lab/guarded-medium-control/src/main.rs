#[cfg(windows)]mod worker_checks;
fn main(){let args:Vec<String>=std::env::args().skip(1).collect();
 if args.is_empty()||args==["--plan"]{println!("{{\"scope\":\"authored_lifecycle_preparation_only\",\"guardWillNotRun\":true,\"targetWillNotRun\":true,\"targetNetworkVerified\":false,\"originalClientImplemented\":false,\"executionReady\":false}}");return}
 #[cfg(windows)]let result=match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice(){
  ["--self-test",pin]=>metallic_guarded_medium_control::digest(pin).and_then(worker_checks::checks),
  ["--death-owner",pin]=>metallic_guarded_medium_control::digest(pin).and_then(worker_checks::death_owner),
  ["--authored-control",launcher,worker,guard]=>metallic_guarded_medium_control::digest(launcher).and_then(|a|metallic_guarded_medium_control::digest(worker).and_then(|b|metallic_guarded_medium_control::digest(guard).and_then(|c|metallic_guarded_medium_control::control::launcher(a,b,c)))),
  _=>Err("closed_authored_roles_required"),
 };
 #[cfg(not(windows))]let result:metallic_guarded_medium_control::Result<()>=Err("windows_only");
 if let Err(reason)=result{eprintln!("{{\"ok\":false,\"error\":\"{reason}\",\"cleanupVerified\":false,\"targetNetworkVerified\":false,\"executionReady\":false}}");std::process::exit(1)}
}
