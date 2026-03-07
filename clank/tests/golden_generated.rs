// @generated — do not edit by hand. Re-run `cargo build` to regenerate.

#[test]
fn golden_builtins_cat_file() {
    clank_golden::run_embedded(
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/builtins/cat-file.yaml")),
        &format!("{}/tests/golden/builtins", env!("CARGO_MANIFEST_DIR")),
    );
}

#[test]
fn golden_builtins_cd_pwd() {
    clank_golden::run_embedded(
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/builtins/cd-pwd.yaml")),
        &format!("{}/tests/golden/builtins", env!("CARGO_MANIFEST_DIR")),
    );
}

#[test]
fn golden_builtins_cp_file() {
    clank_golden::run_embedded(
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/builtins/cp-file.yaml")),
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
fn golden_builtins_env_prints_vars() {
    clank_golden::run_embedded(
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/builtins/env-prints-vars.yaml")),
        &format!("{}/tests/golden/builtins", env!("CARGO_MANIFEST_DIR")),
    );
}

#[test]
fn golden_builtins_head_default() {
    clank_golden::run_embedded(
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/builtins/head-default.yaml")),
        &format!("{}/tests/golden/builtins", env!("CARGO_MANIFEST_DIR")),
    );
}

#[test]
fn golden_builtins_head_n3() {
    clank_golden::run_embedded(
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/builtins/head-n3.yaml")),
        &format!("{}/tests/golden/builtins", env!("CARGO_MANIFEST_DIR")),
    );
}

#[test]
fn golden_builtins_mkdir_basic() {
    clank_golden::run_embedded(
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/builtins/mkdir-basic.yaml")),
        &format!("{}/tests/golden/builtins", env!("CARGO_MANIFEST_DIR")),
    );
}

#[test]
fn golden_builtins_mkdir_parents() {
    clank_golden::run_embedded(
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/builtins/mkdir-parents.yaml")),
        &format!("{}/tests/golden/builtins", env!("CARGO_MANIFEST_DIR")),
    );
}

#[test]
fn golden_builtins_mv_file() {
    clank_golden::run_embedded(
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/builtins/mv-file.yaml")),
        &format!("{}/tests/golden/builtins", env!("CARGO_MANIFEST_DIR")),
    );
}

#[test]
fn golden_builtins_printf_format() {
    clank_golden::run_embedded(
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/builtins/printf-format.yaml")),
        &format!("{}/tests/golden/builtins", env!("CARGO_MANIFEST_DIR")),
    );
}

#[test]
fn golden_builtins_rm_file() {
    clank_golden::run_embedded(
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/builtins/rm-file.yaml")),
        &format!("{}/tests/golden/builtins", env!("CARGO_MANIFEST_DIR")),
    );
}

#[test]
fn golden_builtins_sort_basic() {
    clank_golden::run_embedded(
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/builtins/sort-basic.yaml")),
        &format!("{}/tests/golden/builtins", env!("CARGO_MANIFEST_DIR")),
    );
}

#[test]
fn golden_builtins_sort_reverse() {
    clank_golden::run_embedded(
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/builtins/sort-reverse.yaml")),
        &format!("{}/tests/golden/builtins", env!("CARGO_MANIFEST_DIR")),
    );
}

#[test]
fn golden_builtins_tail_n3() {
    clank_golden::run_embedded(
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/builtins/tail-n3.yaml")),
        &format!("{}/tests/golden/builtins", env!("CARGO_MANIFEST_DIR")),
    );
}

#[test]
fn golden_builtins_touch_creates_file() {
    clank_golden::run_embedded(
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/builtins/touch-creates-file.yaml")),
        &format!("{}/tests/golden/builtins", env!("CARGO_MANIFEST_DIR")),
    );
}

#[test]
fn golden_builtins_uniq_basic() {
    clank_golden::run_embedded(
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/builtins/uniq-basic.yaml")),
        &format!("{}/tests/golden/builtins", env!("CARGO_MANIFEST_DIR")),
    );
}

#[test]
fn golden_builtins_wc_lines() {
    clank_golden::run_embedded(
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/builtins/wc-lines.yaml")),
        &format!("{}/tests/golden/builtins", env!("CARGO_MANIFEST_DIR")),
    );
}

#[test]
fn golden_ls_ls_a() {
    clank_golden::run_embedded(
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/ls/ls-a.yaml")),
        &format!("{}/tests/golden/ls", env!("CARGO_MANIFEST_DIR")),
    );
}

#[test]
fn golden_ls_ls_plain() {
    clank_golden::run_embedded(
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/ls/ls-plain.yaml")),
        &format!("{}/tests/golden/ls", env!("CARGO_MANIFEST_DIR")),
    );
}

#[test]
fn golden_ls_ls_recursive() {
    clank_golden::run_embedded(
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/ls/ls-recursive.yaml")),
        &format!("{}/tests/golden/ls", env!("CARGO_MANIFEST_DIR")),
    );
}

#[test]
fn golden_variables_expand_after_assign() {
    clank_golden::run_embedded(
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/variables/expand-after-assign.yaml")),
        &format!("{}/tests/golden/variables", env!("CARGO_MANIFEST_DIR")),
    );
}

