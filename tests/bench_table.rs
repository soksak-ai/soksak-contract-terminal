// Collects terminal sidecar benchmark reports into one comparison table.
//   SOKSAK_BENCH_OUT=<dir> cargo test -p soksak-contract-terminal --test bench_table -- --ignored --nocapture
//
// This table does not judge. Each sidecar gate enforces the absolute budget independently.
// The removed relative guard compared candidates and let candidates influence the standard.
// 기준을 후보에게 넘긴다(SPEC.md §14.2). 표는 읽으라고 있는 것이지 채점하라고 있는 것이 아니다.
use soksak_contract_terminal::bench::{table, Report};

#[test]
#[ignore]
fn bench_table() {
    let dir = std::env::var("SOKSAK_BENCH_OUT").expect("SOKSAK_BENCH_OUT=<dir>");
    let dir = std::path::PathBuf::from(dir);
    let mut reports: Vec<Report> = std::fs::read_dir(&dir)
        .expect("bench dir")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |x| x == "json"))
        .map(|e| {
            let line = std::fs::read_to_string(e.path()).expect("read");
            Report::from_json(line.trim()).expect("parse")
        })
        .collect();
    assert_eq!(
        reports.len(),
        6,
        "terminal fleet needs six .bench.json reports in {}",
        dir.display()
    );
    let sidecars: std::collections::BTreeSet<_> = reports
        .iter()
        .map(|report| report.sidecar.as_str())
        .collect();
    let expected = std::collections::BTreeSet::from([
        "soksak-sidecar-terminal-alacritty",
        "soksak-sidecar-terminal-ghostty",
        "soksak-sidecar-terminal-kitty",
        "soksak-sidecar-terminal-shitty",
        "soksak-sidecar-terminal-vt100",
        "soksak-sidecar-terminal-wezterm",
    ]);
    assert_eq!(
        sidecars, expected,
        "terminal performance sidecars are incomplete"
    );
    reports.sort_by(|a, b| a.sidecar.cmp(&b.sidecar));
    println!("\n{}", table(&reports));
}
