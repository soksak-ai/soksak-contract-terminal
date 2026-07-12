// 실 데몬 수요 — 예산의 전제("무엇이 데몬의 읽기를 늦추는가")를 사실로 확인한다.
//   SOKSAK_PTYD_BIN=<ptyd> cargo test --release --test daemon_demand -- --ignored --nocapture
use soksak_contract_terminal::daemon_demand as dd;

fn bin() -> std::path::PathBuf {
    dd::ptyd_bin().expect("SOKSAK_PTYD_BIN — scripts/daemon-demand.sh 가 코어에서 빌드해 준다")
}

fn show(a: &dd::Arrival, label: &str) {
    println!(
        "  {label:<22} arrival {:7.1} MB/s   data {:6.1} MB   gap {:6.1} MB   tail {}",
        a.arrival_mb_s,
        a.data_bytes as f64 / 1e6,
        a.gap_bytes as f64 / 1e6,
        if a.tail_seen { "arrived" } else { "LOST" }
    );
}

#[test]
#[ignore]
fn detached_arrival() {
    println!("\n── 분리(detached) — 앱이 없다. 미러가 존재하는 이유가 이 모드다.");
    show(&dd::measure(&bin(), false, None), "구독자=최대속도");
    for (unit, rate) in [("wezterm", 70.0), ("ghostty", 95.0), ("alacritty", 154.0)] {
        show(&dd::measure(&bin(), false, Some(rate)), &format!("{unit} @{rate:.0}MB/s"));
    }
}

#[test]
#[ignore]
fn attached_arrival() {
    println!("\n── 부착(attached) — 프론트가 ack 로 데몬의 PTY 읽기를 늦춘다.");
    show(&dd::measure(&bin(), true, None), "프론트=최대속도ack");
}
