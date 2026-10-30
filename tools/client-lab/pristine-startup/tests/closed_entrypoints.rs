//! Run only authored inert/refusal entrypoints, never the original or high role.
use std::process::Command;
const BINARIES: [&str;2] = [env!("CARGO_BIN_EXE_metallic-pristine-startup"),env!("CARGO_BIN_EXE_metallic-pristine-guard")];

#[test] fn both_actual_entrypoints_refuse_flags_candidate_and_environment_claims() {
    for binary in BINARIES {
        for args in [vec!["--run"],vec!["--one-shot","7","11","22"],vec!["--candidate","approved.json"],vec!["--execution-ready","true"],vec!["--plan","--run"],vec!["--authored-control","ignored"]] {
            let output=Command::new(binary).args(args).env("PRISTINE_EXECUTION_READY","true").env("PRISTINE_ORIGINAL_TARGET_NETWORK_VERIFIED","true").env("PRISTINE_SOURCE_APPROVED","true").output().unwrap();
            assert_eq!(output.status.code(),Some(1)); assert!(output.stdout.is_empty());
            assert_eq!(String::from_utf8(output.stderr).unwrap().trim(),r#"{"ok":false,"error":"original_target_network_and_source_admission_missing","gameWillNotRun":true,"guardWillNotRun":true,"executionReady":false}"#);
        }
    }
}

#[test] fn both_actual_default_and_plan_are_explicitly_inert() {
    for binary in BINARIES { for args in [vec![],vec!["--plan"]] {
        let output=Command::new(binary).args(args).output().unwrap(); assert!(output.status.success());assert!(output.stderr.is_empty());
        assert_eq!(String::from_utf8(output.stdout).unwrap().trim(),metallic_pristine_startup::PLAN);
    }}
}
