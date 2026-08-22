use soksak_contract_terminal::bench::{Report, BENCHMARK_REPORT_SPEC};

#[test]
fn benchmark_report_is_versioned_json() {
    let report = Report {
        sidecar: "soksak-sidecar-terminal-vt100".into(),
        feed_mb_s: 120.5,
        rehydrate_ms: 1.2,
        paint_bytes: 4096,
        cold_ms: 2.3,
        cold_bytes: 2048,
        live_bytes: 1024,
        rss_bytes: 8192,
        demand_mb_s: 75.0,
        gap_bytes: 0,
        tail_seen: true,
    };
    let json = report.to_json();
    assert!(json.contains(BENCHMARK_REPORT_SPEC));
    let parsed = Report::from_json(&json).expect("parse benchmark JSON");
    assert!(json.contains("\"sidecar\":\"soksak-sidecar-terminal-vt100\""));
    assert!(!json.contains("\"unit\""));
    assert_eq!(parsed.sidecar, "soksak-sidecar-terminal-vt100");
    assert_eq!(parsed.feed_mb_s, 120.5);
    assert_eq!(parsed.gap_bytes, 0);
    assert!(parsed.tail_seen);
}

#[test]
fn benchmark_report_rejects_another_spec() {
    let json = r#"{"spec":"other@0.0.1"}"#;
    assert!(Report::from_json(json).is_err());
}
