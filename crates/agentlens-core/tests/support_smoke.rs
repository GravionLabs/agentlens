mod support;

use support::TempHome;

#[test]
fn temp_home_lays_out_fixtures_under_a_home_dir() {
    let home = TempHome::new();
    let copied = home.copy_fixture("sample.txt", ".claude/projects/demo/sample.txt");
    assert!(copied.starts_with(home.path()));
    assert_eq!(
        std::fs::read_to_string(copied).unwrap().trim(),
        "synthetic fixture"
    );
    assert_eq!(home.env().home(), home.path());
}
