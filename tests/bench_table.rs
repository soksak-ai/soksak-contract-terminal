// 벤치 비교표 — 유닛들이 낸 측정 줄을 모아 하나의 표로 찍는다. 표를 만드는 곳도 계약 하나다.
//   SOKSAK_BENCH_OUT=<dir> cargo test -p soksak-contract-terminal --test bench_table -- --ignored --nocapture
//
// **여기에 판정은 없다.** 예산은 유닛 게이트가 이미 강제했고(수요 대비 비율, SPEC.md §14.2), 그
// 판정은 유닛 하나로 완결된다 — 다른 유닛이 무엇을 냈는지 볼 필요가 없기 때문이다. 예전에는 이 표가
// "같은 실행 최고 유닛의 ¼" 이라는 상대 가드를 강제했다. 그 가드는 폐기했다: 후보끼리 견주는 판정은
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
