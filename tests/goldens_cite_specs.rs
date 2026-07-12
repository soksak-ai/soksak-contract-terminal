// 골든의 저자는 규격이어야 한다 — 엔진이 아니라.
//
// 골든을 "엔진들이 이렇게 하더라"로 정당화하면, 넷이 똑같이 틀렸을 때 그 오답이 골든이 된다.
// 그래서 계약은 골든의 **논거에 엔진 이름이 등장하는 것 자체를** 금지한다(SPEC.md §11.A·§12).
// 이 시험이 그 금지의 유일한 강제 지점이다 — 사람의 눈은 다음 골든에서 반드시 진다.
//
// 소문자 `vt100` 은 크레이트 이름이라 금지어다. DEC 단말을 인용할 때는 대문자로 — "DEC VT100
// User Guide" 처럼 문서를 지목한다(문서는 권위 서열 2, 크레이트는 권위 아님).

use std::path::PathBuf;

/// 골든의 논거에 나타나서는 안 되는 이름들 — 엔진, 엔진 크레이트, 그리고 유닛 자신.
const BANNED: [&str; 7] = [
    "alacritty",
    "wezterm",
    "ghostty",
    "libghostty",
    "alacritty_terminal",
    "wezterm-term",
    "soksak-sidecar-terminal",
];

fn goldens() -> Vec<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("goldens");
    let mut v: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("goldens/")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map_or(false, |x| x == "golden"))
        .collect();
    v.sort();
    assert!(!v.is_empty(), "골든이 하나도 없다: {}", dir.display());
    v
}

#[test]
fn no_engine_authors_a_golden() {
    let mut sins: Vec<String> = Vec::new();
    for path in goldens() {
        let text = std::fs::read_to_string(&path).expect("read golden");
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        for (n, line) in text.lines().enumerate() {
            // 논거는 주석에 산다. 데이터 줄에는 산문이 없다.
            if !line.starts_with('#') {
                continue;
            }
            let lower = line.to_lowercase();
            for banned in BANNED {
                if lower.contains(banned) {
                    sins.push(format!("{name}:{}: `{banned}` — {}", n + 1, line.trim()));
                }
            }
            // 소문자 vt100 = 크레이트. 대문자 VT100 = DEC 문서(허용).
            if line.contains("vt100") {
                sins.push(format!(
                    "{name}:{}: 소문자 `vt100` 은 크레이트 이름이다. DEC 문서를 인용하려면 \
                     대문자 VT100 으로 — {}",
                    n + 1,
                    line.trim()
                ));
            }
        }
    }
    assert!(
        sins.is_empty(),
        "골든의 논거가 엔진을 인용한다(SPEC.md §11.A: 엔진은 어느 층위에서도 권위가 아니다):\n  {}",
        sins.join("\n  ")
    );
}

#[test]
fn every_golden_cites_a_specification() {
    // 산문만 있고 인용이 없는 골든은 미완이다 — 그 값이 어디서 왔는지 아무도 모른다.
    // 규격을 인용했거나(문서 이름), 침묵을 선언했거나(§11.S 표의 항목) 둘 중 하나여야 한다.
    const GROUNDS: [&str; 6] = ["ctlseqs", "xterm", "ECMA-48", "UAX #11", "Unicode", "§11.S"];
    let mut ungrounded: Vec<String> = Vec::new();
    for path in goldens() {
        let text = std::fs::read_to_string(&path).expect("read golden");
        let head: String = text.lines().take_while(|l| l.starts_with('#')).collect::<Vec<_>>().join("\n");
        if !GROUNDS.iter().any(|g| head.contains(g)) {
            ungrounded.push(path.file_name().unwrap().to_string_lossy().to_string());
        }
    }
    assert!(
        ungrounded.is_empty(),
        "이 골든들의 논거에 규격 인용도 침묵 선언(§11.S)도 없다 — 값의 출처가 없다:\n  {}",
        ungrounded.join("\n  ")
    );
}
