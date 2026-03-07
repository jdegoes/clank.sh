// @generated — do not edit by hand. Re-run `cargo build` to regenerate.

#[test]
fn golden_builtins_cd_pwd() {
    clank_golden::run_embedded(
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/builtins/cd-pwd.yaml")),
        &format!("{}/tests/golden/builtins", env!("CARGO_MANIFEST_DIR")),
    );
}

#[test]
fn golden_builtins_echo_hello() {
    clank_golden::run_embedded(
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/builtins/echo-hello.yaml")),
        &format!("{}/tests/golden/builtins", env!("CARGO_MANIFEST_DIR")),
    );
}

#[test]
fn golden_variables_expand_after_assign() {
    clank_golden::run_embedded(
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/variables/expand-after-assign.yaml")),
        &format!("{}/tests/golden/variables", env!("CARGO_MANIFEST_DIR")),
    );
}

