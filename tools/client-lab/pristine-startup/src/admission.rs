use crate::Result;
pub const BLOCKED: &str = "original_target_network_and_source_admission_missing";
/// Private field; no CLI, environment, receipt or candidate can create this.
pub(crate) struct Permit { _private: () }
pub(crate) fn require_execution() -> Result<Permit> { Err(BLOCKED) }

#[cfg(test)] mod tests {
    #[test] fn pristine_no_runtime_assertion_can_provide_admission() {
        for _ in 0..4 { assert!(matches!(super::require_execution(),Err(super::BLOCKED))); }
    }
}
