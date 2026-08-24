use soksak_contract_terminal::bench::{BENCHMARK_REPORT_SPEC, Report};

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
    };
    let json = report.to_json();
    assert!(json.contains(BENCHMARK_REPORT_SPEC));
    let parsed = Report::from_json(&json).expect("parse benchmark JSON");
    assert!(json.contains("\"sidecar\":\"soksak-sidecar-terminal-vt100\""));
    assert!(!json.contains("\"unit\""));
    assert_eq!(parsed.sidecar, "soksak-sidecar-terminal-vt100");
    assert_eq!(parsed.feed_mb_s, 120.5);
    assert_eq!(BENCHMARK_REPORT_SPEC, "soksak-spec-terminal-benchmark@0.0.2");
    for composition_field in ["demandMbS", "gapBytes", "tailSeen"] {
        assert!(!json.contains(composition_field));
    }
}

#[test]
fn benchmark_report_rejects_another_spec() {
    let json = r#"{"spec":"other@0.0.1"}"#;
    assert!(Report::from_json(json).is_err());
}
