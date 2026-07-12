// 벤치 비교표 — 유닛들이 낸 측정 줄을 모아 하나의 표로 찍는다. 표를 만드는 곳도 계약 하나다.
//   SOKSAK_BENCH_OUT=<dir> cargo test -p soksak-contract-terminal --test bench_table -- --ignored --nocapture
use soksak_contract_terminal::bench::{table, Report};

#[test]
#[ignore]
fn bench_table() {
    let dir = std::env::var("SOKSAK_BENCH_OUT").expect("SOKSAK_BENCH_OUT=<dir>");
    let dir = std::path::PathBuf::from(dir);
    let mut reports: Vec<Report> = std::fs::read_dir(&dir)
        .expect("bench dir")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |x| x == "bench"))
        .map(|e| {
            let line = std::fs::read_to_string(e.path()).expect("read");
            Report::from_line(line.trim()).expect("parse")
        })
        .collect();
    assert!(!reports.is_empty(), "no .bench files in {}", dir.display());
    reports.sort_by(|a, b| a.unit.cmp(&b.unit));
    println!("\n{}", table(&reports));
}
