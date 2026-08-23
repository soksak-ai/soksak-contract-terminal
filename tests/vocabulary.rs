use std::path::{Path, PathBuf};

const LEGACY_TERM: &str = "golden";

#[test]
fn rust_package_uses_edition_2024() {
    let manifest = std::fs::read_to_string("Cargo.toml").expect("read Cargo.toml");
    assert!(manifest.lines().any(|line| line == r#"edition = "2024""#));
}

#[test]
fn verification_uses_exact_tools() {
    let workflow =
        std::fs::read_to_string(".github/workflows/test.yml").expect("read verification workflow");
    assert!(workflow.contains("actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1"));
    assert!(workflow.contains("dtolnay/rust-toolchain@4be7066ada62dd38de10e7b70166bc74ed198c30"));
    assert!(workflow.contains("toolchain: \"1.96.0\""));
}

fn public_files(root: &Path) -> Vec<PathBuf> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(path) = pending.pop() {
        for entry in std::fs::read_dir(path).expect("read contract directory") {
            let path = entry.expect("read contract entry").path();
            if path
                .file_name()
                .is_some_and(|name| name == "target" || name == ".git")
            {
                continue;
            }
            if path.is_dir() {
                pending.push(path);
            } else {
                files.push(path);
            }
        }
    }
    files
}

#[test]
fn public_contract_uses_reference_state_vocabulary() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut violations = Vec::new();
    for path in public_files(&root) {
        let relative = path.strip_prefix(&root).expect("contract path");
        if relative == Path::new("tests/vocabulary.rs") {
            continue;
        }
        if relative
            .to_string_lossy()
            .to_lowercase()
            .contains(LEGACY_TERM)
        {
            violations.push(relative.display().to_string());
            continue;
        }
        if let Ok(text) = std::fs::read_to_string(&path)
            && text.to_lowercase().contains(LEGACY_TERM)
        {
            violations.push(relative.display().to_string());
        }
    }
    violations.sort();
    assert!(
        violations.is_empty(),
        "public contract contains retired terminology:\n{}",
        violations.join("\n")
    );
}
