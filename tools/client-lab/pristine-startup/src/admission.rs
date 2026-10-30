use crate::Result;
pub const BLOCKED: &str = "original_target_network_and_source_admission_missing";
/// Only the fixed-role dispatcher receives this owned value. It is not a receipt assertion.
pub(crate) struct Permit { _private: () }
pub(crate) fn require_execution() -> Result<Permit> { Err(BLOCKED) }

#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) struct ImagePins {
    pub(crate) launcher: [u8; 32],
    pub(crate) guard: [u8; 32],
}

#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) enum Request {
    Launcher(ImagePins),
    Guard { nonce: [u8; 16], parent: crate::protocol::Stamp, pins: ImagePins },
}

fn pins(launcher: &str, guard: &str) -> Result<ImagePins> {
    Ok(ImagePins {
        launcher: crate::digest(launcher).map_err(|_| BLOCKED)?,
        guard: crate::digest(guard).map_err(|_| BLOCKED)?,
    })
}

fn nonce(text: &str) -> Result<[u8; 16]> {
    if text.len() != 32 || !text.bytes().all(|b| b.is_ascii_hexdigit()) { return Err(BLOCKED); }
    let mut value = [0; 16];
    for (i, byte) in value.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).map_err(|_| BLOCKED)?;
    }
    if value == [0; 16] { return Err(BLOCKED); }
    Ok(value)
}

fn decimal(text: &str, max_chars: usize) -> Result<u64> {
    if text.is_empty() || text.len() > max_chars || text.starts_with('0') || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(BLOCKED);
    }
    text.parse::<u64>().map_err(|_| BLOCKED)
}

pub(crate) fn parse(entry: crate::Entry, args: &[String]) -> Result<Request> {
    match entry {
        crate::Entry::Launcher if args.len() == 3 && args[0] == "--observe-no-authored-section" => {
            Ok(Request::Launcher(pins(&args[1], &args[2])?))
        }
        crate::Entry::Guard if args.len() == 6 && args[0] == "--guard-observation" => {
            let parent = crate::protocol::Stamp {
                pid: u32::try_from(decimal(&args[2], 10)?).map_err(|_| BLOCKED)?,
                created: decimal(&args[3], 20)?,
            };
            if !parent.valid() { return Err(BLOCKED); }
            Ok(Request::Guard { nonce: nonce(&args[1])?, parent, pins: pins(&args[4], &args[5])? })
        }
        _ => Err(BLOCKED),
    }
}

// Syntax selects only a reviewed fixed operation; all live image/token/peer/guard
// checks still belong to control. No path, command or asserted permission enters here.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn admit_fixed(request: Request) -> (Permit, Request) {
    (Permit { _private: () }, request)
}

#[cfg(test)] mod tests {
    use super::*;
    fn launcher() -> Vec<String> { ["--observe-no-authored-section".into(), "11".repeat(32), "22".repeat(32)].into() }
    fn guard() -> Vec<String> { ["--guard-observation".into(), "07".repeat(16), "1".into(), "2".into(), "11".repeat(32), "22".repeat(32)].into() }
    fn refused(entry: crate::Entry, args: &[String]) { assert!(matches!(parse(entry, args), Err(BLOCKED))); }
    #[test] fn pristine_no_runtime_assertion_can_provide_admission() {
        for _ in 0..4 { assert!(matches!(super::require_execution(),Err(super::BLOCKED))); }
    }
    #[test] fn pristine_fixed_launcher_parser_preserves_exact_pins() {
        match parse(crate::Entry::Launcher, &launcher()).unwrap_or_else(|_| panic!("fixed launcher refused")) {
            Request::Launcher(pins) => { assert_eq!(pins.launcher, [0x11; 32]); assert_eq!(pins.guard, [0x22; 32]); }
            _ => panic!("wrong fixed role"),
        }
    }
    #[test] fn pristine_fixed_guard_parser_preserves_nonce_and_parent() {
        match parse(crate::Entry::Guard, &guard()).unwrap_or_else(|_| panic!("fixed guard refused")) {
            Request::Guard { nonce, parent, pins } => {
                assert_eq!(nonce, [7; 16]); assert_eq!(parent, crate::protocol::Stamp { pid: 1, created: 2 });
                assert_eq!(pins.launcher, [0x11; 32]); assert_eq!(pins.guard, [0x22; 32]);
            }
            _ => panic!("wrong fixed role"),
        }
    }
    #[test] fn pristine_parser_enforces_entry_role_and_exact_arity() {
        refused(crate::Entry::Guard, &launcher()); refused(crate::Entry::Launcher, &guard());
        for mut args in [launcher(), guard()] {
            args.push("extra".into()); refused(crate::Entry::Launcher, &args); refused(crate::Entry::Guard, &args);
        }
        for n in 0..3 { refused(crate::Entry::Launcher, &launcher()[..n]); }
        for n in 0..6 { refused(crate::Entry::Guard, &guard()[..n]); }
    }
    #[test] fn pristine_parser_has_no_flag_path_json_or_environment_fallback() {
        for flag in ["--run", "--one-shot", "--plan", "--candidate", "--execution-ready", "--OBSERVE-NO-AUTHORED-SECTION", "true", "{}", "C:\\Other.exe"] {
            let mut args = launcher(); args[0] = flag.into(); refused(crate::Entry::Launcher, &args);
            let mut args = guard(); args[0] = flag.into(); refused(crate::Entry::Guard, &args);
        }
    }
    #[test] fn pristine_parser_rejects_every_malformed_pin_position() {
        for bad in ["".into(), "0".repeat(63), "0".repeat(65), "gg".repeat(32), "é".repeat(32), format!("{}\n", "1".repeat(63))] {
            for i in [1, 2] { let mut args = launcher(); args[i] = bad.clone(); refused(crate::Entry::Launcher, &args); }
            for i in [4, 5] { let mut args = guard(); args[i] = bad.clone(); refused(crate::Entry::Guard, &args); }
        }
        let mut args = launcher(); args[1] = "AB".repeat(32); args[2] = "0".repeat(64);
        assert!(matches!(parse(crate::Entry::Launcher, &args), Ok(Request::Launcher(_))));
    }
    #[test] fn pristine_parser_requires_exact_nonzero_nonce() {
        for bad in ["".into(), "0".repeat(32), "1".repeat(31), "1".repeat(33), "zz".repeat(16), "é".repeat(16), format!("{} ", "1".repeat(31))] {
            let mut args = guard(); args[1] = bad; refused(crate::Entry::Guard, &args);
        }
        let mut args = guard(); args[1] = "AF".repeat(16);
        assert!(matches!(parse(crate::Entry::Guard, &args), Ok(Request::Guard { .. })));
    }
    #[test] fn pristine_parser_requires_positive_canonical_decimal_words() {
        for bad in ["", "0", "00", "01", "+1", "-1", " 1", "1 ", "1\n", "1.0", "0x1", "１"] {
            for i in [2, 3] { let mut args = guard(); args[i] = bad.into(); refused(crate::Entry::Guard, &args); }
        }
        for (i, bad) in [(2, "4294967296"), (2, "18446744073709551615"), (3, "18446744073709551616")] {
            let mut args = guard(); args[i] = bad.into(); refused(crate::Entry::Guard, &args);
        }
        let mut args = guard(); args[2] = u32::MAX.to_string(); args[3] = u64::MAX.to_string();
        assert!(matches!(parse(crate::Entry::Guard, &args), Ok(Request::Guard { .. })));
    }
    #[test] fn pristine_fixed_admission_preserves_owned_request() {
        let request = parse(crate::Entry::Launcher, &launcher()).unwrap_or_else(|_| panic!("fixed launcher refused"));
        let (_permit, request) = admit_fixed(request);
        assert!(matches!(request, Request::Launcher(_)));
        assert!(require_execution().is_err());
    }
}
