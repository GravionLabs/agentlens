//! The Windows installers put the install directory on PATH: the NSIS hook file
//! (`bundle.windows.nsis.installerHooks`) and the WiX fragment
//! (`bundle.windows.wix.fragmentPaths`) are wired into the bundle configuration
//! and keep the rules of the PATH edit.
//!
//! When `makensis` is on PATH the hook file is compiled in every install mode of
//! the Tauri template; set `AGENTLENS_REQUIRE_MAKENSIS=1` (CI does) to fail
//! instead of skipping when it is missing.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

// ---- helpers: configuration and files ----

fn manifest_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn config() -> Value {
    let text = std::fs::read_to_string(manifest_dir().join("tauri.conf.json"))
        .expect("tauri.conf.json is readable");
    serde_json::from_str(&text).expect("tauri.conf.json is valid JSON")
}

fn hook_path() -> PathBuf {
    let config = config();
    let hooks = config["bundle"]["windows"]["nsis"]["installerHooks"]
        .as_str()
        .expect("installerHooks is set");
    manifest_dir().join(hooks)
}

fn hook_text() -> String {
    std::fs::read_to_string(hook_path()).expect("hook file exists")
}

fn wix_path() -> PathBuf {
    let config = config();
    let fragment = config["bundle"]["windows"]["wix"]["fragmentPaths"][0]
        .as_str()
        .expect("a WiX fragment is configured");
    manifest_dir().join(fragment)
}

fn wix_text() -> String {
    std::fs::read_to_string(wix_path()).expect("WiX fragment exists")
}

// ---- helpers: NSIS source ----

/// The body of `!macro <name> ...` up to its `!macroend`.
fn macro_body<'a>(text: &'a str, name: &str) -> &'a str {
    let head = format!("!macro {name}");
    let start = text
        .lines()
        .scan(0, |offset, line| {
            let at = *offset;
            *offset += line.len() + 1;
            Some((at, line))
        })
        .find(|(_, line)| line.trim() == head || line.trim().starts_with(&format!("{head} ")))
        .map(|(at, _)| at)
        .unwrap_or_else(|| panic!("macro {name} is defined"));
    let rest = &text[start..];
    let end = rest.find("!macroend").expect("macro is closed");
    &rest[..end]
}

/// The statements of a piece of NSIS source: trimmed, without comments and blank lines.
fn statements(text: &str) -> Vec<&str> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with(';'))
        .collect()
}

fn position(statements: &[&str], wanted: impl Fn(&str) -> bool, what: &str) -> usize {
    statements
        .iter()
        .position(|s| wanted(s))
        .unwrap_or_else(|| panic!("statement not found: {what}"))
}

fn edit_function() -> String {
    macro_body(&hook_text(), "AgentLensDefineEditPath").to_owned()
}

// ---- helpers: makensis ----

fn makensis() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .flat_map(|dir| [dir.join("makensis"), dir.join("makensis.exe")])
        .find(|candidate| candidate.is_file())
}

/// A minimal stand-in for Tauri's installer.nsi: the hook file is included before
/// INSTALLMODE is defined (as in the template), the hooks run in the install and
/// the uninstall section, and `$MultiUser.InstallMode` exists only in "both" mode.
fn wrapper(mode: &str, hooks: &Path, out: &Path, uninstall_body: &str) -> String {
    let multi_user = if mode == "both" {
        "Var MultiUser.InstallMode\nFunction .onInit\n  StrCpy $MultiUser.InstallMode \"CurrentUser\"\nFunctionEnd\n"
    } else {
        ""
    };
    format!(
        "Unicode true\nOutFile \"{out}\"\nRequestExecutionLevel user\n{multi_user}\
         !include \"{hooks}\"\n!define INSTALLMODE \"{mode}\"\n\
         Section\n  !insertmacro NSIS_HOOK_POSTINSTALL\n  WriteUninstaller \"$INSTDIR\\uninstall.exe\"\nSectionEnd\n\
         Section Uninstall\n{uninstall_body}\nSectionEnd\n",
        out = out.display(),
        hooks = hooks.display(),
    )
}

/// Compiles a wrapper with warnings as errors; returns success and the output.
fn compile(makensis: &Path, script: &str) -> (bool, String) {
    let dir = tempfile::tempdir().expect("temp dir");
    let file = dir.path().join("wrapper.nsi");
    std::fs::write(&file, script).expect("wrapper written");
    let output = Command::new(makensis)
        .args(["-WX", "-V2", "-INPUTCHARSET", "UTF8"])
        .arg(&file)
        .output()
        .expect("makensis runs");
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    (output.status.success(), log)
}

fn require_makensis() -> Option<PathBuf> {
    let found = makensis();
    if found.is_none() {
        if std::env::var_os("AGENTLENS_REQUIRE_MAKENSIS").is_some() {
            panic!("makensis is not on PATH but AGENTLENS_REQUIRE_MAKENSIS is set");
        }
        eprintln!("skipped: makensis is not on PATH, the NSIS hook file is not compiled");
    }
    found
}

// ---- NSIS: compiled ----

#[test]
fn hook_file_compiles_in_every_install_mode() {
    let Some(makensis) = require_makensis() else {
        return;
    };
    let out = tempfile::tempdir().expect("temp dir");
    for mode in ["currentUser", "perMachine", "both"] {
        let script = wrapper(
            mode,
            &hook_path(),
            &out.path().join(format!("{mode}.exe")),
            "  !insertmacro NSIS_HOOK_POSTUNINSTALL",
        );
        let (ok, log) = compile(&makensis, &script);
        assert!(ok, "{mode}: makensis failed:\n{log}");
    }
}

#[test]
fn compile_check_rejects_an_installer_function_in_the_uninstaller() {
    // proves the check would catch the mistake of calling a plain function from
    // the uninstall section
    let Some(makensis) = require_makensis() else {
        return;
    };
    let out = tempfile::tempdir().expect("temp dir");
    let script = wrapper(
        "currentUser",
        &hook_path(),
        &out.path().join("bad.exe"),
        "  Push \"remove\"\n  Push \"HKCU\"\n  Call AgentLensEditPath",
    );
    let (ok, log) = compile(&makensis, &script);
    assert!(!ok, "makensis accepted the script:\n{log}");
    assert!(log.contains("un."), "unexpected error:\n{log}");
}

// ---- NSIS: wiring and structure ----

#[test]
fn hook_file_is_referenced_and_defines_both_hooks() {
    let text = hook_text();
    macro_body(&text, "NSIS_HOOK_POSTINSTALL");
    macro_body(&text, "NSIS_HOOK_POSTUNINSTALL");
    assert!(text.contains("!include \"WinMessages.nsh\""));
    assert!(text.contains("!include \"LogicLib.nsh\""));
}

#[test]
fn edit_function_exists_for_installer_and_uninstaller() {
    let text = hook_text();
    let defines: Vec<_> = statements(&text)
        .into_iter()
        .filter(|s| s.starts_with("!insertmacro AgentLensDefineEditPath"))
        .collect();
    assert!(defines.contains(&"!insertmacro AgentLensDefineEditPath \"\""));
    assert!(defines.contains(&"!insertmacro AgentLensDefineEditPath \"un.\""));
    assert!(edit_function().contains("Function ${un}AgentLensEditPath"));
    assert!(macro_body(&text, "AgentLensEditPathCall").contains("Call ${un}AgentLensEditPath"));

    let install = macro_body(&text, "NSIS_HOOK_POSTINSTALL");
    let uninstall = macro_body(&text, "NSIS_HOOK_POSTUNINSTALL");
    assert!(install.contains("!insertmacro AgentLensPath \"\" \"add\""));
    assert!(uninstall.contains("!insertmacro AgentLensPath \"un.\" \"remove\""));
}

#[test]
fn section_macros_leave_registers_alone() {
    // the hooks run inside the template's sections, which use $0 and friends
    let text = hook_text();
    for name in [
        "NSIS_HOOK_POSTINSTALL",
        "NSIS_HOOK_POSTUNINSTALL",
        "AgentLensPath",
        "AgentLensEditPathCall",
    ] {
        for statement in statements(macro_body(&text, name)) {
            let writes = [
                "Pop ",
                "StrCpy ",
                "Exch ",
                "IntOp ",
                "StrLen ",
                "ReadRegStr ",
            ]
            .iter()
            .any(|op| statement.starts_with(op));
            assert!(!writes, "{name} writes a register: {statement}");
        }
    }
}

#[test]
fn edit_function_restores_every_register_it_saves() {
    let body = edit_function();
    let statements = statements(&body);
    let mut saved: Vec<&str> = statements
        .iter()
        .filter_map(|s| s.strip_prefix("Exch $"))
        .map(|r| r.split_whitespace().next().unwrap())
        .collect();
    let done = position(&statements, |s| s == "done:", "done label");
    saved.extend(
        statements[..done]
            .iter()
            .take_while(|s| !s.starts_with("${"))
            .filter_map(|s| s.strip_prefix("Push $")),
    );
    let restored: Vec<&str> = statements[done + 1..]
        .iter()
        .filter_map(|s| s.strip_prefix("Pop $"))
        .collect();
    saved.reverse();
    assert_eq!(restored, saved);
}

#[test]
fn every_skip_says_why() {
    let body = edit_function();
    let statements = statements(&body);
    let skips: Vec<usize> = statements
        .iter()
        .enumerate()
        .filter(|(_, s)| **s == "Goto done")
        .map(|(i, _)| i)
        .collect();
    assert!(!skips.is_empty());
    for i in skips {
        assert!(
            statements[i - 1].starts_with("DetailPrint \"AgentLens: "),
            "skip without a message after: {}",
            statements[i - 1]
        );
    }
}

#[test]
fn install_directory_with_separator_or_percent_is_rejected_before_reading() {
    let body = edit_function();
    let statements = statements(&body);
    let semicolon = position(&statements, |s| s == "${If} $6 == \";\"", "check for ';'");
    let percent = position(&statements, |s| s == "${OrIf} $6 == \"%\"", "check for '%'");
    let first_read = position(&statements, |s| s.starts_with("ReadRegStr"), "PATH read");
    assert_eq!(percent, semicolon + 1);
    assert!(percent < first_read);
    // the scan walks $INSTDIR, not the PATH value
    assert!(statements[..semicolon].contains(&"StrCpy $6 $INSTDIR 1 $5"));
}

#[test]
fn machine_path_needs_admin_and_program_files() {
    let body = edit_function();
    let statements = statements(&body);
    let hklm = position(&statements, |s| s == "${If} $R9 == \"HKLM\"", "HKLM branch");
    let admin = position(&statements, |s| s == "${If} $0 != \"Admin\"", "admin check");
    let files = position(
        &statements,
        |s| s == "StrCpy $2 $PROGRAMFILES",
        "Program Files",
    );
    let files64 = position(
        &statements,
        |s| s == "StrCpy $2 $PROGRAMFILES64",
        "Program Files 64",
    );
    let under = position(
        &statements,
        |s| s == "${AndIf} $7 == \"\\\"",
        "separator check",
    );
    let not_under = position(&statements, |s| s == "${If} $3 == 0", "outside check");
    let read = position(
        &statements,
        |s| s.starts_with("ReadRegStr $0 HKLM"),
        "machine PATH read",
    );
    assert!(hklm < admin && admin < files && files < files64);
    assert!(files64 < under && under < not_under && not_under < read);
    assert_eq!(statements[admin - 1], "Pop $0");
    assert_eq!(statements[admin - 2], "UserInfo::GetAccountType");
}

#[test]
fn path_near_the_nsis_string_limit_is_left_untouched() {
    let body = edit_function();
    let statements = statements(&body);
    let read = position(
        &statements,
        |s| s.starts_with("ReadRegStr $0 HKCU"),
        "user read",
    );
    let old_len = position(&statements, |s| s == "StrLen $7 $0", "old length");
    let old_guard = position(&statements, |s| s == "${If} $7 >= 1023", "old guard");
    let split = position(&statements, |s| s == "${While} $1 != \"\"", "split loop");
    assert!(read < old_len && old_len + 1 == old_guard && old_guard < split);

    let new_guard = position(&statements, |s| s == "${If} $7 > 1023", "new guard");
    let append = position(&statements, |s| s == "StrCpy $2 \"$2;$INSTDIR\"", "append");
    let write = position(&statements, |s| s.starts_with("WriteRegExpandStr"), "write");
    assert!(split < new_guard && new_guard < append && append < write);
}

#[test]
fn existing_entry_is_marked_as_found() {
    // without this an update would append the directory a second time
    let body = edit_function();
    let statements = statements(&body);
    let branch = position(
        &statements,
        |s| s == "${ElseIf} $4 == $INSTDIR",
        "existing entry branch",
    );
    assert_eq!(statements[branch + 1], "StrCpy $3 1");
}

#[test]
fn install_mode_variable_is_used_only_in_both_mode() {
    let text = hook_text();
    let path_macro = macro_body(&text, "AgentLensPath");
    let statements = statements(path_macro);
    let both = position(
        &statements,
        |s| s == "!else if \"${INSTALLMODE}\" == \"both\"",
        "both branch",
    );
    let end = both + position(&statements[both..], |s| s == "!else", "branch after both");
    let uses: Vec<usize> = statements
        .iter()
        .enumerate()
        .filter(|(_, s)| s.contains("$MultiUser.InstallMode"))
        .map(|(i, _)| i)
        .collect();
    assert!(!uses.is_empty());
    assert!(
        uses.iter().all(|&i| both < i && i < end),
        "used outside the both branch"
    );
    // and nowhere else in the file
    let elsewhere = statements_outside(&text, path_macro);
    assert!(!elsewhere.contains("$MultiUser.InstallMode"));
}

fn statements_outside(text: &str, part: &str) -> String {
    statements(&text.replacen(part, "", 1)).join("\n")
}

#[test]
fn environment_change_is_broadcast() {
    let text = hook_text();
    let path_macro = macro_body(&text, "AgentLensPath");
    assert!(statements(path_macro).contains(
        &"SendMessage ${HWND_BROADCAST} ${WM_SETTINGCHANGE} 0 \"STR:Environment\" /TIMEOUT=5000"
    ));
}

#[test]
fn existing_bundle_settings_are_kept() {
    let bundle = &config()["bundle"];
    assert_eq!(bundle["externalBin"].as_array().map(Vec::len), Some(2));
    assert_eq!(bundle["active"], true);
}

// ---- WiX ----

#[test]
fn wix_fragment_is_included_and_its_component_referenced() {
    let config = config();
    let wix = &config["bundle"]["windows"]["wix"];
    assert_eq!(
        wix["fragmentPaths"],
        serde_json::json!(["windows/path.wxs"])
    );
    assert_eq!(
        wix["componentRefs"],
        serde_json::json!(["AgentLensUserPath"])
    );
    let text = wix_text();
    let doc = roxmltree::Document::parse(&text).expect("fragment is valid XML");
    let component = doc
        .descendants()
        .find(|n| n.has_tag_name("Component") && n.attribute("Id") == Some("AgentLensUserPath"))
        .expect("referenced component exists");
    let parent = component.parent_element().expect("component has a parent");
    assert!(parent.has_tag_name("DirectoryRef"));
    assert_eq!(parent.attribute("Id"), Some("INSTALLDIR"));
}

#[test]
fn wix_fragment_sets_the_user_path_and_removes_it_on_uninstall() {
    let text = wix_text();
    let doc = roxmltree::Document::parse(&text).expect("fragment is valid XML");
    let component = doc
        .descendants()
        .find(|n| n.has_tag_name("Component"))
        .expect("component");
    let environment = component
        .children()
        .find(|n| n.has_tag_name("Environment"))
        .expect("Environment element in the component");
    for (name, value) in [
        ("Name", "PATH"),
        ("Value", "[INSTALLDIR]"),
        ("Action", "set"),
        ("Part", "last"),
        ("System", "no"),
        ("Permanent", "no"),
    ] {
        assert_eq!(
            environment.attribute(name),
            Some(value),
            "Environment/@{name}"
        );
    }

    // a per-user component needs an HKCU key path
    let key = component
        .children()
        .find(|n| n.has_tag_name("RegistryValue"))
        .expect("RegistryValue in the component");
    assert_eq!(key.attribute("Root"), Some("HKCU"));
    assert_eq!(key.attribute("KeyPath"), Some("yes"));
}

#[test]
fn wix_fragment_has_a_fixed_guid() {
    let text = wix_text();
    let doc = roxmltree::Document::parse(&text).expect("fragment is valid XML");
    let guid = doc
        .descendants()
        .find(|n| n.has_tag_name("Component"))
        .and_then(|n| n.attribute("Guid"))
        .expect("component has a Guid");
    let groups: Vec<usize> = guid.split('-').map(str::len).collect();
    assert_eq!(groups, [8, 4, 4, 4, 12], "{guid}");
    assert!(
        guid.chars().all(|c| c.is_ascii_hexdigit() || c == '-'),
        "{guid}"
    );
}
