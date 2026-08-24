use std::fs;

#[test]
fn owner_benchmark_does_not_execute_another_sidecar() {
    let bench = fs::read_to_string("src/bench.rs").expect("read bench source");
    let lib = fs::read_to_string("src/lib.rs").expect("read library source");
    for forbidden in ["SOKSAK_PTYD_BIN", "ptyd_bin", "daemon_demand", "gap_bytes", "tail_seen"] {
        assert!(!bench.contains(forbidden), "owner benchmark contains {forbidden}");
    }
    assert!(!lib.contains("pub mod daemon_demand"));
    assert!(!std::path::Path::new("tests/daemon_demand.rs").exists());
}
