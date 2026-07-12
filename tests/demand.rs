// 수요 — 미러가 따라잡아야 하는 상대를 잰다(SPEC.md §14). feed 예산은 후보의 실측이 아니라
// 이 숫자에서 나온다. 평시 시험에 끼지 않는다(느리고, 기계를 독점해야 정확하다).
//   cargo test --release --test demand -- --ignored --nocapture
use soksak_contract_terminal::demand;

// 프론트 터미널의 소비율을 재려면 그 프론트에 **같은 코퍼스**를 먹여야 한다(다른 것을 먹인
// 숫자는 비교 대상이 아니다). scripts/frontend-demand.sh 가 이 테스트로 코퍼스를 꺼내 간다.
//   SOKSAK_CORPUS_OUT=<path> cargo test --release --test demand -- --ignored dump_corpus
#[test]
#[ignore]
fn dump_corpus() {
    let path = std::env::var("SOKSAK_CORPUS_OUT").expect("SOKSAK_CORPUS_OUT=<path>");
    let corpus = soksak_contract_terminal::bench::corpus();
    std::fs::write(&path, &corpus).expect("write corpus");
    println!("{} bytes -> {path}", corpus.len());
}

#[test]
#[ignore]
fn demand() {
    let ceiling = demand::pty_ceiling_mb_s();
    let tee = demand::tee_delivery_mb_s();
    println!("\n  PTY ceiling   {ceiling:8.1} MB/s   (생산의 절대 상한 — 어떤 셸도 이보다 빠를 수 없다)");
    println!("  tee delivery  {tee:8.1} MB/s   <<< 수요. feed 예산이 곧 이 값이다(SPEC.md §14.2)");
    assert!(ceiling > 0.0 && tee > 0.0, "수요가 0 이면 측정이 실패한 것이다");
    assert!(tee <= ceiling * 1.05, "tee 배달률이 PTY 천장을 넘을 수는 없다 — 측정이 틀렸다");
}
