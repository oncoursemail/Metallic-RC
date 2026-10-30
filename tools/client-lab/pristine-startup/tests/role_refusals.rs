//! Never execute a valid self-pinned role here: only syntax refusals or zero self-pins.
use std::process::Command;
const LAUNCHER: &str = env!("CARGO_BIN_EXE_metallic-pristine-startup");
const GUARD: &str = env!("CARGO_BIN_EXE_metallic-pristine-guard");

fn refused(binary: &str, args: &[String]) {
    let output = Command::new(binary).args(args).output().expect("owned refusal child");
    assert_eq!(output.status.code(), Some(1), "fixed role did not return its normal refusal exit");
    assert!(output.stdout.is_empty(), "refused role exported a success body");
    let text = String::from_utf8(output.stderr).expect("static UTF-8 error");
    assert!(text.starts_with("{\"ok\":false,") && text.trim_end().ends_with('}'), "static refusal envelope required");
    assert!(text.contains("\"executionReady\":false"), "refusal must preserve unverified readiness");
    assert!(!text.contains("\"ok\":true") && !text.contains("\"startupVerified\":true") && !text.contains("\"executionReady\":true"));
}

#[test]
fn malformed_fixed_roles_and_cross_entry_roles_refuse() {
    let zero = "0".repeat(64);
    let launcher = vec!["--observe-no-authored-section".into(), zero.clone(), zero.clone()];
    let guard = vec!["--guard-observation".into(), "07".repeat(16), "1".into(), "2".into(), zero.clone(), zero.clone()];
    refused(GUARD, &launcher); refused(LAUNCHER, &guard);
    for binary in [LAUNCHER, GUARD] {
        for args in [vec!["--observe-no-authored-section".into()], vec!["--guard-observation".into()],
            vec!["--observe-no-authored-section".into(), zero.clone(), zero.clone(), "extra".into()]] {
            refused(binary, &args);
        }
    }
    for (index, bad) in [(1, "00".repeat(16)), (2, "+1".into()), (2, "4294967296".into()), (3, "18446744073709551616".into()), (4, "not-a-pin".into())] {
        let mut args = guard.clone(); args[index] = bad; refused(GUARD, &args);
    }
}

#[test]
fn zero_self_pins_cannot_reach_original_or_elevated_work() {
    let zero = "0".repeat(64);
    refused(LAUNCHER, &["--observe-no-authored-section".into(), zero.clone(), zero.clone()]);
    refused(GUARD, &["--guard-observation".into(), "07".repeat(16), "1".into(), "2".into(), zero.clone(), zero]);
}

#[cfg(not(windows))]
#[test]
fn non_windows_fixed_role_syntax_remains_unsupported() {
    let launcher = "11".repeat(32); let guard = "22".repeat(32);
    refused(LAUNCHER, &["--observe-no-authored-section".into(), launcher.clone(), guard.clone()]);
    refused(GUARD, &["--guard-observation".into(), "07".repeat(16), "1".into(), "2".into(), launcher, guard]);
}
