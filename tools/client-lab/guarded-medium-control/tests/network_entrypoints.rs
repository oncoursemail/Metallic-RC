//! Only inert plans and malformed active arguments. Never invokes a valid
//! guarded role, UAC, WFP, worker, original target or user-supplied executable.
use std::{
    path::PathBuf,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

fn invoke(name: &str, args: &[&str]) -> std::process::Output {
    let suffix = if cfg!(windows) { ".exe" } else { "" };
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/release")
        .join(format!("{name}{suffix}"));
    let mut command = Command::new(path);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command
        .spawn()
        .expect("build the reviewed release binaries first");
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        if child.try_wait().expect("owned status").is_some() {
            return child.wait_with_output().expect("bounded inert output");
        }
        if Instant::now() >= until {
            let _ = child.kill();
            let _ = child.wait();
            panic!("inert/refusal command exceeded its deadline")
        }
        thread::sleep(Duration::from_millis(5));
    }
}
fn refused(name: &str, args: &[&str]) {
    let output = invoke(name, args);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.len() <= 1024);
    let error = String::from_utf8(output.stderr).unwrap();
    assert!(error.contains("\"ok\":false"));
    assert!(error.contains("\"executionReady\":false"));
}
#[test]
fn control_plan_and_malformed_network_dispatch_are_inert() {
    let plan = invoke("metallic-medium-control", &["--plan"]);
    assert!(plan.status.success());
    assert!(plan.stderr.is_empty());
    assert_eq!(String::from_utf8(plan.stdout).unwrap().trim(),"{\"scope\":\"authored_lifecycle_preparation_only\",\"guardWillNotRun\":true,\"targetWillNotRun\":true,\"targetNetworkVerified\":false,\"originalClientImplemented\":false,\"executionReady\":false}");
    for args in [
        vec!["--network-control"],
        vec!["--network-control", "not-hex", "not-hex", "not-hex"],
        vec!["--target", "RecRoom.exe"],
        vec![
            "--network-control",
            "not-hex",
            "not-hex",
            "not-hex",
            "extra",
        ],
    ] {
        refused("metallic-medium-control", &args);
    }
}
#[test]
fn guard_plan_and_malformed_network_dispatch_are_inert() {
    let plan = invoke("metallic-medium-guard", &["--plan"]);
    assert!(plan.status.success());
    assert!(plan.stderr.is_empty());
    assert_eq!(String::from_utf8(plan.stdout).unwrap().trim(),"{\"guardWillNotRun\":true,\"originalClientImplemented\":false,\"targetNetworkVerified\":false,\"executionReady\":false}");
    let zero = "00000000000000000000000000000000";
    let nonce = "11111111111111111111111111111111";
    let pin = "0000000000000000000000000000000000000000000000000000000000000000";
    for args in [
        vec!["--network-one-shot"],
        vec!["--network-one-shot", zero, "1", "1", pin, pin, pin],
        vec!["--network-one-shot", nonce, "0", "1", pin, pin, pin],
        vec!["--network-one-shot", nonce, "1", "0", pin, pin, pin],
        vec!["--network-one-shot", nonce, "-1", "1", pin, pin, pin],
    ] {
        refused("metallic-medium-guard", &args);
    }
}
