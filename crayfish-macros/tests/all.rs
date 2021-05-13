#[test]
fn test_macros() {
    let t = trybuild::TestCases::new();
    t.pass("tests/trybuild/args_ok.rs");
    t.compile_fail("tests/trybuild/args_err.rs");
}
