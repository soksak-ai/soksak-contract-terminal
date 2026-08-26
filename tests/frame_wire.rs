use serde_json::Value;
use soksak_contract_terminal::Fixture;
use soksak_contract_terminal::frame::{self, CUT_POINTS};

#[test]
fn every_fixture_has_a_frame_reply_series() {
    for fixture in Fixture::ALL {
        let series = frame::load_series(fixture.stem());
        assert_eq!(series.fixture, fixture.stem());
        assert_eq!(series.replies.len(), CUT_POINTS, "{}: replies", fixture.stem());
        assert!(series.replies[0].full, "{}: first reply is full", fixture.stem());
        assert!(
            series.cuts.windows(2).all(|pair| pair[0] < pair[1]),
            "{}: cuts ascend",
            fixture.stem()
        );
        assert_eq!(
            series.cuts,
            frame::cut_points(fixture.stream().len()).to_vec(),
            "{}: cuts follow the corpus",
            fixture.stem()
        );
    }
}

#[test]
fn frame_replies_use_camel_case_keys_only() {
    for fixture in Fixture::ALL {
        let text = std::fs::read_to_string(frame::series_path(fixture.stem())).unwrap();
        let value: Value = serde_json::from_str(&text).unwrap();
        let mut offenders = Vec::new();
        collect_offending_keys(&value["replies"], "replies", &mut offenders);
        assert!(
            offenders.is_empty(),
            "{}: keys that are not camelCase:\n  {}",
            fixture.stem(),
            offenders.join("\n  ")
        );
    }
}

#[test]
fn applying_the_series_reproduces_the_declared_reference_state() {
    for fixture in Fixture::ALL {
        frame::assert_series_reproduces(&frame::load_series(fixture.stem()), fixture);
    }
}

fn collect_offending_keys(value: &Value, path: &str, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let mut chars = key.chars();
                let camel = chars.next().is_some_and(|c| c.is_ascii_lowercase())
                    && chars.all(|c| c.is_ascii_alphanumeric());
                if !camel {
                    out.push(format!("{path}.{key}"));
                }
                collect_offending_keys(child, &format!("{path}.{key}"), out);
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                collect_offending_keys(item, &format!("{path}[{index}]"), out);
            }
        }
        _ => {}
    }
}
