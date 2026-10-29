fn main(){let args:Vec<String>=std::env::args().skip(1).collect();
 if args.is_empty()||args==["--plan"]{println!("{{\"guardWillNotRun\":true,\"originalClientImplemented\":false,\"targetNetworkVerified\":false,\"executionReady\":false}}");return}
 #[cfg(windows)]let result=(||->metallic_guarded_medium_control::Result<()>{let a:Vec<&str>=args.iter().map(String::as_str).collect();match a.as_slice(){
  ["--one-shot",nonce,pid,created,launcher,worker,guard]=>{let n=metallic_guarded_medium_control::control::parse_nonce(nonce)?;let stamp=metallic_guarded_medium_control::protocol::Stamp{pid:pid.parse().map_err(|_|"parent_pid_required")?,created:created.parse().map_err(|_|"parent_time_required")?};if !stamp.valid(){return Err("parent_stamp_required")}metallic_guarded_medium_control::control::guard(n,stamp,metallic_guarded_medium_control::digest(launcher)?,metallic_guarded_medium_control::digest(worker)?,metallic_guarded_medium_control::digest(guard)?)},
  _=>Err("closed_guard_role_required"),
 }})();
 #[cfg(not(windows))]let result:metallic_guarded_medium_control::Result<()>=Err("windows_only");
 if let Err(reason)=result{eprintln!("{{\"ok\":false,\"error\":\"{reason}\",\"cleanupVerified\":false,\"executionReady\":false}}");std::process::exit(1)}
}
