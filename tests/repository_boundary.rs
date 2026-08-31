use std::fs;
use std::path::Path;

#[test]
fn contract_tests_do_not_name_provider_repositories() {
    let provider_prefix = ["soksak", "sidecar", "terminal"].join("-") + "-";
    for entry in fs::read_dir("tests").expect("read tests") {
        let path = entry.expect("test entry").path();
        if path == Path::new("tests/repository_boundary.rs") || path.extension().is_none_or(|ext| ext != "rs") {
            continue;
        }
        let source = fs::read_to_string(&path).expect("read test source");
        assert!(
            !source.contains(&provider_prefix),
            "{} names a provider repository; contract tests use neutral fixtures",
            path.display()
        );
    }
}
