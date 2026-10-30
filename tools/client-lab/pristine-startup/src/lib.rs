//! Closed source candidate for one pristine installed-build startup observation.
pub type Result<T> = std::result::Result<T, &'static str>;
pub const ENVIRONMENT: [&str; 20] = ["SystemRoot", "SystemDrive", "WINDIR", "PATH", "COMSPEC", "TEMP", "TMP", "LOCALAPPDATA", "APPDATA", "USERPROFILE", "HOMEDRIVE", "HOMEPATH", "ProgramData", "ProgramFiles", "ProgramFiles(x86)", "PUBLIC", "USERNAME", "USERDOMAIN", "NUMBER_OF_PROCESSORS", "PROCESSOR_ARCHITECTURE"];
pub const PLAN: &str = r#"{"scope":"pristine_startup_source_candidate","steamBuildId":"23191908","variant":"no-authored-section","executorSourcePrepared":true,"gameWillNotRun":true,"guardWillNotRun":true,"targetNetworkVerified":false,"originalClientTested":false,"startupVerified":false,"executionReady":false,"blockingReasons":["original_target_guard_profile_not_verified","guard_receipt_provenance_not_verified","independent_original_executor_source_admission_missing"]}"#;

pub mod admission;
pub mod originals;
// Exact reviewed implementation from820e23f, not a second mutable toolkit.
#[path = "../../guarded-medium-control/src/protocol.rs"] pub mod protocol;
#[path = "../../guarded-medium-control/src/pe.rs"] pub mod pe;
#[cfg(windows)] #[path = "../../guarded-medium-control/src/ipc.rs"] mod ipc;
#[cfg(windows)] #[path = "../../guarded-medium-control/src/wfp.rs"] mod wfp;
#[cfg(windows)] #[allow(dead_code)] mod process;
#[cfg(windows)] #[allow(dead_code)] mod control;

pub fn digest(text: &str) -> Result<[u8; 32]> {
    if text.len() != 64 || !text.bytes().all(|c| c.is_ascii_hexdigit()) { return Err("reviewed_image_sha256_required"); }
    let mut result = [0; 32];
    for (i, value) in result.iter_mut().enumerate() { *value = u8::from_str_radix(&text[i*2..i*2+2],16).map_err(|_|"reviewed_image_sha256_required")?; }
    Ok(result)
}

pub fn environment_block(rows: impl IntoIterator<Item=(String, String)>) -> Result<Vec<u16>> {
    let mut selected = std::collections::BTreeMap::new();
    for (name,value) in rows {
        if let Some(expected) = ENVIRONMENT.iter().find(|key| key.eq_ignore_ascii_case(&name)) {
            if value.contains('\0') || selected.insert(expected.to_ascii_uppercase(),(expected.to_string(),value)).is_some() { return Err("ambiguous_environment"); }
        }
    }
    if !matches!(selected.get("SYSTEMDRIVE"),Some((_,drive)) if drive.len()==2 && drive.as_bytes()[0].is_ascii_alphabetic() && drive.as_bytes()[1]==b':') { return Err("systemdrive_required"); }
    let mut block = Vec::new();
    for (_, (name,value)) in selected { block.extend(format!("{name}={value}").encode_utf16()); block.push(0); }
    block.push(0);
    Ok(block)
}

/// Mere CLI data cannot construct an execution permit. No active route exists.
pub fn closed_entrypoint(args: &[String]) -> Result<&'static str> {
    if args.is_empty() || args == ["--plan"] { return Ok(PLAN); }
    admission::require_execution().map(|_| PLAN)
}

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum Observation { RunningBeforeCleanup, ExitedCleanly, Crashed(u32) }
pub fn observation(before_cleanup: Option<u32>) -> Observation {
    match before_cleanup { None => Observation::RunningBeforeCleanup, Some(0) => Observation::ExitedCleanly, Some(code) => Observation::Crashed(code) }
}
pub(crate) fn remaining_observation(elapsed: std::time::Duration) -> Result<std::time::Duration> {
    let remaining=std::time::Duration::from_secs(80).checked_sub(elapsed).ok_or("original_startup_budget_exhausted")?;
    if remaining.is_zero() { return Err("original_startup_budget_exhausted"); }
    Ok(remaining)
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn pristine_every_active_cli_shape_remains_blocked() {
        for args in [vec!["--run"],vec!["--one-shot","anything"],vec!["--candidate","approved.json"],vec!["--execution-ready","true"],vec!["--authored-control"],vec!["--plan","--run"]] {
            assert_eq!(closed_entrypoint(&args.into_iter().map(String::from).collect::<Vec<_>>()),Err(admission::BLOCKED));
        }
        assert_eq!(closed_entrypoint(&[]),Ok(PLAN));
        assert_eq!(closed_entrypoint(&["--plan".into()]),Ok(PLAN));
    }
    #[test] fn pristine_environment_requires_systemdrive_and_excludes_secret_names() {
        assert!(environment_block([("PATH".into(),"synthetic".into())]).is_err());
        for drive in ["", "C", "C:\\", "C:extra", "1:"] { assert!(environment_block([("SystemDrive".into(),drive.into())]).is_err()); }
        let b=environment_block([("SystemDrive".into(),"C:".into()),("TEMP".into(),"synthetic-猫".into()),("SECRET_TOKEN".into(),"omit".into())]).unwrap();
        assert_eq!(&b[b.len()-2..],&[0,0]); let text=String::from_utf16(&b).unwrap(); assert!(text.contains("TEMP=synthetic-猫")); assert!(!text.contains("SECRET"));assert!(!text.contains("omit"));
        assert!(environment_block([("SystemDrive".into(),"C:".into()),("SYSTEMDRIVE".into(),"C:".into())]).is_err());
    }
    #[test] fn pristine_running_or_crashed_is_never_inferred_from_cleanup_exit() {
        assert_eq!(observation(None),Observation::RunningBeforeCleanup);
        assert_eq!(observation(Some(0)),Observation::ExitedCleanly);
        assert_eq!(observation(Some(0xc0000005)),Observation::Crashed(0xc0000005));
        assert_eq!(observation(Some(124)),Observation::Crashed(124));
    }
    #[test] fn pristine_expired_protected_budget_refuses_before_resume() {
        use std::time::Duration;
        assert_eq!(remaining_observation(Duration::ZERO),Ok(Duration::from_secs(80)));
        assert_eq!(remaining_observation(Duration::from_secs(79)),Ok(Duration::from_secs(1)));
        for seconds in [80,81,90,130] { assert_eq!(remaining_observation(Duration::from_secs(seconds)),Err("original_startup_budget_exhausted")); }
    }
}
