//! Starting Harlo at login, each test in its own scratch autostart folder.

// Rust guideline compliant 2026-10-09

use std::path::Path;

use harlo_core::autostart;

#[test]
fn turning_it_on_and_off_writes_and_removes_the_entry() {
    let scratch = tempfile::tempdir().expect("a scratch folder");
    let folder = scratch.path().join("autostart");
    let program = Path::new("/opt/harlo/bin/harlo");
    assert!(!autostart::enabled(&folder));

    autostart::set(&folder, program, true).expect("on");
    assert!(autostart::enabled(&folder));
    let text = std::fs::read_to_string(folder.join("harlo.desktop")).expect("written");
    assert!(text.starts_with("[Desktop Entry]\n"), "{text}");
    assert!(text.contains("\nType=Application\n"));
    assert!(text.contains("\nName=Harlo\n"));
    assert!(text.contains("\nExec=/opt/harlo/bin/harlo\n"), "{text}");

    autostart::set(&folder, program, false).expect("off");
    assert!(!autostart::enabled(&folder));
    autostart::set(&folder, program, false).expect("off again is fine");
}

#[test]
fn a_program_path_with_spaces_and_signs_is_quoted() {
    let entry = autostart::entry(Path::new("/home/a b/100% \"x\"/harlo"));
    let exec = entry
        .lines()
        .find_map(|line| line.strip_prefix("Exec="))
        .expect("an Exec line");
    // Quoted with `"` and `\"`, then `\` doubled as a string value, and `%`
    // doubled as a field code.
    assert_eq!(exec, r#""/home/a b/100%% \\"x\\"/harlo""#);
}
