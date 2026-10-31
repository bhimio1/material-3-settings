use super::*;
use std::fs;

fn fixture(shell: &str) -> String {
    fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/shells").join(shell).join("shell.qml")).unwrap()
}

/// A fake home with one shell installed from its fixture.
fn home_with(t: Target) -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().to_path_buf();
    let dir = t.dir(&home);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("shell.qml"), fixture(t.id())).unwrap();
    (tmp, home)
}

#[test]
fn every_pack_parses() {
    for pack in ["ii", "inir"] {
        let hooks = pack_hooks(pack).unwrap();
        assert!(hooks.iter().any(|h| h.feature == "core"), "{pack} has no core hooks");
        assert!(pack_files(pack).iter().any(|(p, _)| *p == "m3/M3Root.qml"));
    }
    assert!(pack_files("dux").iter().any(|(p, _)| *p == "Material3/plugin.json"));
}

#[test]
fn applies_to_each_upstream_and_removes_cleanly() {
    for t in [Target::Ii, Target::End4Pc, Target::Inir] {
        let (_tmp, home) = home_with(t);
        let dir = t.dir(&home);
        let original = fixture(t.id());
        assert_eq!(status(&home, t).state, State::Clean);

        let report = apply(&home, t).unwrap();
        assert_eq!(report.skipped, vec![], "{}", t.id());
        assert!(report.features.contains(&"core".to_string()));
        let shell = fs::read_to_string(dir.join("shell.qml")).unwrap();
        assert!(shell.contains("import \"m3\"\n"), "{}", t.id());
        assert!(shell.contains("    M3Root {}\n"), "{}", t.id());
        assert!(dir.join("m3/M3Root.qml").is_file());
        assert_eq!(status(&home, t).state, State::Applied { pack_version: PACK_VERSION });

        // Applying again changes nothing.
        let before = fs::read_to_string(dir.join("shell.qml")).unwrap();
        let again = apply(&home, t).unwrap();
        assert_eq!(again.written, 0, "{}", t.id());
        assert_eq!(fs::read_to_string(dir.join("shell.qml")).unwrap(), before);

        remove(&home, t).unwrap();
        assert_eq!(fs::read_to_string(dir.join("shell.qml")).unwrap(), original, "{}", t.id());
        assert!(!dir.join("m3").exists());
        assert_eq!(status(&home, t).state, State::Clean);
    }
}

#[test]
fn an_update_that_replaces_files_needs_reapply() {
    let (_tmp, home) = home_with(Target::End4Pc);
    let dir = Target::End4Pc.dir(&home);
    apply(&home, Target::End4Pc).unwrap();
    // The updater puts upstream's shell.qml back.
    fs::write(dir.join("shell.qml"), fixture("end4-pC")).unwrap();
    assert_eq!(status(&home, Target::End4Pc).state, State::NeedsReapply);
    let done = reapply_all(&home);
    assert_eq!(done.len(), 1);
    assert!(fs::read_to_string(dir.join("shell.qml")).unwrap().contains("M3Root {}"));
    assert_eq!(status(&home, Target::End4Pc).state, State::Applied { pack_version: PACK_VERSION });
}

#[test]
fn a_missing_anchor_leaves_the_shell_untouched() {
    let (_tmp, home) = home_with(Target::Ii);
    let dir = Target::Ii.dir(&home);
    fs::write(dir.join("shell.qml"), "import QtQuick\nScope {}\n").unwrap();
    let report = apply(&home, Target::Ii).unwrap();
    assert!(report.features.is_empty());
    assert_eq!(report.skipped.len(), 2);
    assert_eq!(fs::read_to_string(dir.join("shell.qml")).unwrap(), "import QtQuick\nScope {}\n");
    assert!(!dir.join("m3").exists());
}

#[test]
fn a_feature_with_one_missing_hook_is_left_out_whole() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    fs::write(dir.join("shell.qml"), "ShellRoot {\n}\n").unwrap();
    fs::write(dir.join("Other.qml"), "Item {\n}\n").unwrap();
    let hooks = hooks::parse(
        r#"
[[hook]]
id = "root"
feature = "core"
file = "shell.qml"
anchor = '^ShellRoot \{'
action = "after"
text = "    M3Root {}"

[[hook]]
id = "games-a"
feature = "games"
file = "shell.qml"
anchor = '^\}'
action = "before"
text = "    Games {}"

[[hook]]
id = "games-b"
feature = "games"
file = "Other.qml"
anchor = 'NotThere'
action = "after"
text = "x"

[[hook]]
id = "pc-only"
feature = "about"
file = "Other.qml"
anchor = '^Item'
action = "after"
text = "    About {}"
only = ["end4-pC"]
"#,
    )
    .unwrap();
    let p = plan(dir, "ii", &hooks).unwrap();
    assert_eq!(p.features, vec!["core".to_string()]);
    assert_eq!(p.skipped.len(), 1);
    assert_eq!(p.skipped[0].feature, "games");
    assert_eq!(p.edits["shell.qml"], "ShellRoot {\n    // m3:begin root\n    M3Root {}\n    // m3:end root\n}\n");
    assert!(!p.edits.contains_key("Other.qml"));

    let p = plan(dir, "end4-pC", &hooks).unwrap();
    assert_eq!(p.features, vec!["about".to_string(), "core".to_string()]);
}

#[test]
fn dux_plugin_is_written_and_enabled() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().to_path_buf();
    // Pretend Dux Shell is installed in the user's config.
    fs::create_dir_all(home.join(".config/quickshell/dms")).unwrap();
    fs::write(home.join(".config/quickshell/dms/shell.qml"), "").unwrap();
    fs::create_dir_all(home.join(".config/dms")).unwrap();
    fs::write(home.join(".config/dms/plugin_settings.json"), r#"{"other":{"enabled":true}}"#).unwrap();

    apply(&home, Target::Dux).unwrap();
    assert!(Target::Dux.dir(&home).join("plugin.json").is_file());
    let s: serde_json::Value = serde_json::from_str(&fs::read_to_string(home.join(".config/dms/plugin_settings.json")).unwrap()).unwrap();
    assert_eq!(s["material3"]["enabled"], true);
    assert_eq!(s["other"]["enabled"], true);
    assert_eq!(status(&home, Target::Dux).state, State::Applied { pack_version: PACK_VERSION });

    remove(&home, Target::Dux).unwrap();
    assert!(!Target::Dux.dir(&home).exists());
    let s: serde_json::Value = serde_json::from_str(&fs::read_to_string(home.join(".config/dms/plugin_settings.json")).unwrap()).unwrap();
    assert!(s.get("material3").is_none());
}

#[test]
fn targets_parse_from_common_names() {
    assert_eq!(Target::parse("end4-pC"), Some(Target::End4Pc));
    assert_eq!(Target::parse("dms"), Some(Target::Dux));
    assert_eq!(Target::parse("iNiR"), Some(Target::Inir));
    assert_eq!(Target::parse("nope"), None);
}
