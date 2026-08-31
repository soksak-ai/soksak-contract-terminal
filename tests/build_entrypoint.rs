use std::fs;
use std::path::Path;

#[test]
fn contract_owns_one_make_entrypoint_and_no_provider_gate() {
    let makefile = fs::read_to_string("Makefile").expect("Makefile must exist");
    for target in ["preflight:", "prepare:", "build:", "verify:"] {
        assert!(makefile.contains(target), "Makefile omits {target}");
    }
    for duplicate in ["RUST_VERSION :=", "TARGETS :="] {
        assert!(
            !makefile.contains(duplicate),
            "Makefile duplicates declarative metadata: {duplicate}"
        );
    }
    assert!(Path::new("rust-toolchain.toml").is_file());
    assert!(
        !Path::new("scripts/gate.sh").exists(),
        "contract repository must not execute provider repositories"
    );
}

#[test]
fn workflow_injects_the_rust_owner_and_calls_make() {
    let workflow = fs::read_to_string(".github/workflows/test.yml").unwrap();
    assert!(workflow.contains("rust-toolchain.toml"));
    assert!(workflow.contains("make verify"));
    assert!(!workflow.contains("toolchain: \"1.96.0\""));
    assert!(!workflow.contains("cargo test --release"));
}

#[test]
fn locked_build_owns_a_versioned_lockfile() {
    let makefile = fs::read_to_string("Makefile").expect("Makefile must exist");
    assert!(
        makefile.contains("cargo fetch --locked")
            && makefile.contains("cargo build --locked")
            && makefile.contains("cargo test --locked"),
        "every Cargo build phase must preserve the locked dependency closure"
    );

    assert!(
        Path::new("Cargo.lock").is_file(),
        "a repository that verifies with --locked must version Cargo.lock"
    );

    let gitignore = fs::read_to_string(".gitignore").expect(".gitignore must exist");
    assert!(
        !gitignore.lines().any(|line| line.trim() == "Cargo.lock"),
        "Cargo.lock cannot reach a clean checkout while .gitignore excludes it"
    );
}
