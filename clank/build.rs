use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let golden_dir = PathBuf::from(&manifest_dir).join("tests/golden");
    let out = PathBuf::from(&manifest_dir).join("tests/golden_generated.rs");

    // Tell cargo to re-run this script if any golden YAML changes.
    println!("cargo:rerun-if-changed=tests/golden");

    let mut tests = String::new();
    tests.push_str("// @generated — do not edit by hand. Re-run `cargo build` to regenerate.\n\n");

    if golden_dir.exists() {
        collect_tests(&golden_dir, &golden_dir, &mut tests);
    }

    fs::write(&out, &tests).unwrap_or_else(|e| panic!("failed to write {}: {e}", out.display()));
}

fn collect_tests(root: &Path, dir: &Path, out: &mut String) {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("failed to read dir {}: {e}", dir.display()))
        .filter_map(|e| e.ok())
        .collect();
    entries.sort_by_key(|e| e.path());

    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            // Skip the setup/ directory — those files are not test cases.
            if path.file_name().map(|n| n == "setup").unwrap_or(false) {
                continue;
            }
            collect_tests(root, &path, out);
        } else if path.extension().map(|e| e == "yaml").unwrap_or(false) {
            emit_test(root, &path, out);
        }
    }
}

fn emit_test(root: &Path, yaml_path: &Path, out: &mut String) {
    // Derive a valid Rust identifier from the relative path.
    // e.g. "builtins/echo-hello.yaml" -> "golden_builtins_echo_hello"
    let rel = yaml_path
        .strip_prefix(root)
        .unwrap_or(yaml_path)
        .with_extension("");
    let test_name = format!(
        "golden_{}",
        rel.to_string_lossy().replace(['/', '\\', '-', '.'], "_")
    );

    // Paths relative to CARGO_MANIFEST_DIR for portability.
    // root = <manifest>/tests/golden, manifest = root.parent().parent()
    let manifest = root.parent().and_then(|p| p.parent()).unwrap_or(root);

    let rel_yaml = yaml_path
        .strip_prefix(manifest)
        .unwrap_or(yaml_path)
        .to_string_lossy()
        .into_owned();

    let rel_base = yaml_path
        .parent()
        .unwrap_or(root)
        .strip_prefix(manifest)
        .unwrap_or(yaml_path.parent().unwrap_or(root))
        .to_string_lossy()
        .into_owned();

    out.push_str(&format!(
        r#"#[test]
fn {test_name}() {{
    clank_golden::run_embedded(
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/{rel_yaml}")),
        &format!("{{}}/{rel_base}", env!("CARGO_MANIFEST_DIR")),
    );
}}

"#
    ));
}
