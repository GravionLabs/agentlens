//! The NSIS installer hook file is wired into the bundle configuration.

use serde_json::Value;
use std::path::Path;

fn config() -> Value {
    let text =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tauri.conf.json"))
            .expect("tauri.conf.json is readable");
    serde_json::from_str(&text).expect("tauri.conf.json is valid JSON")
}

#[test]
fn nsis_hook_file_is_referenced_and_exists() {
    let config = config();
    let hooks = config["bundle"]["windows"]["nsis"]["installerHooks"]
        .as_str()
        .expect("installerHooks is set");
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join(hooks);
    let text = std::fs::read_to_string(&file).expect("hook file exists");
    assert!(text.contains("!macro NSIS_HOOK_POSTINSTALL"));
    assert!(text.contains("!macro NSIS_HOOK_POSTUNINSTALL"));
    assert!(text.contains("WriteRegExpandStr"));
    assert!(text.contains("WM_SETTINGCHANGE"));
}

fn hook_text() -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("windows/hooks.nsh"))
        .expect("hook file exists")
}

#[test]
fn hook_leaves_a_path_near_the_nsis_string_limit_untouched() {
    let text = hook_text();
    // the old value is measured before anything is rebuilt, and the new one before it is written
    assert!(text.contains("StrLen $7 $0"));
    assert!(text.contains("${If} $7 >= 1023"));
    assert!(text.contains("${If} $7 > 1023"));
    assert!(text.contains("Goto done"));
    assert!(text.contains("done:"));
}

#[test]
fn hook_uses_the_install_mode_variable_only_in_both_mode() {
    let text = hook_text();
    let both = text
        .find("!else if \"${INSTALLMODE}\" == \"both\"")
        .expect("both branch");
    let var = text
        .find("${If} $MultiUser.InstallMode")
        .expect("variable used");
    assert!(
        var > both,
        "the variable is only referenced in the both branch"
    );
    assert_eq!(text.matches("$MultiUser.InstallMode").count(), 2); // one in a comment
    assert!(text.contains("!if \"${INSTALLMODE}\" == \"perMachine\""));
}

#[test]
fn hook_marks_an_existing_entry_as_found() {
    // without this an update would append the directory a second time
    assert!(hook_text().contains("StrCpy $3 1"));
}

#[test]
fn existing_bundle_settings_are_kept() {
    let bundle = &config()["bundle"];
    assert_eq!(bundle["externalBin"].as_array().map(Vec::len), Some(2));
    assert_eq!(bundle["active"], true);
}
