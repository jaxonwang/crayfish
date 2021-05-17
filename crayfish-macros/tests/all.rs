#[test]
fn test_macros() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/trybuild/main_no_async.rs");
    t.compile_fail("tests/trybuild/args_err.rs");
    t.compile_fail("tests/trybuild/ret_infer_err.rs");
}
