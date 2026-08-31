// Formats caller-supplied benchmark reports with the contract schema.
//   SOKSAK_BENCH_OUT=<dir> cargo test -p soksak-contract-terminal --test bench_table -- --ignored --nocapture
//
// This test validates formatting only. Provider inventory and performance acceptance belong to the
// installed-system test repository.
use soksak_contract_terminal::bench::{Report, table};

#[test]
#[ignore]
fn bench_table() {
    let dir = std::env::var("SOKSAK_BENCH_OUT").expect("SOKSAK_BENCH_OUT=<dir>");
    let dir = std::path::PathBuf::from(dir);
    let mut reports: Vec<Report> = std::fs::read_dir(&dir)
        .expect("bench dir")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .map(|e| {
            let line = std::fs::read_to_string(e.path()).expect("read");
            Report::from_json(line.trim()).expect("parse")
        })
        .collect();
    assert!(!reports.is_empty(), "benchmark directory contains no reports: {}", dir.display());
    reports.sort_by(|a, b| a.sidecar.cmp(&b.sidecar));
    println!("\n{}", table(&reports));
}
