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

#[test]
fn existing_bundle_settings_are_kept() {
    let bundle = &config()["bundle"];
    assert_eq!(bundle["externalBin"].as_array().map(Vec::len), Some(2));
    assert_eq!(bundle["active"], true);
}
