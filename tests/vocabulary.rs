use std::path::{Path, PathBuf};

const LEGACY_TERM: &str = "golden";

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
