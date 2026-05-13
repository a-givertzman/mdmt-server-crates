#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.pass("tests/ui/test_success.rs");
    t.compile_fail("tests/ui/fail_bad_suffix.rs");
    t.compile_fail("tests/ui/fail_only_suffix.rs");
}