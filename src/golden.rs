//! 골든 코덱 — 선언된 화면 상태를 사람이 읽고 리뷰할 수 있는 텍스트로 담고 되읽는다.
//!
//! 포맷은 데이터 파일이지 언어가 아니다(줄 하나 = 값 하나, 행 하나 = 칸들). 각 행 앞에 그 행의
//! 평문을 주석으로 달아, 기계용 칸 목록과 사람이 읽는 화면을 같은 파일에서 나란히 본다.
//!
//! ```text
//! cols 80
//! rows 24
//! alt 0
//! cursor 3 5                       # x y (표시 여부는 modes.show_cursor)
//! modes 0 0 0 0 0 0 0 0 0 1 1 1 0  # SPEC.md §11 의 13 모드 순서
//! history 2
//! visible 24
//! # |BORDER-BOX|
//! V00 42:n:-:-:- 4F:n:-:-:- ...
//! ```
//!
//! 칸 토큰은 `<코드포인트>:<폭>:<fg>:<bg>:<속성>` 다섯 칸이고, 빈 기본 칸은 `.` 로 줄인다.
//! 같은 칸이 이어지면 `N*<토큰>` 으로 접는다. 색은 `-`(기본)·`pNN`(팔레트)·`RRGGBB`(트루컬러),
//! 속성은 `-` 또는 `b d i u v s h`(bold/dim/italic/underline/inverse/strikeout/hidden)의 부분집합.

use std::path::PathBuf;

use crate::state::{Attrs, Cell, Color, Modes, Row, ScreenState};

/// 골든 파일을 읽어 선언된 화면 상태를 돌려준다. 파일이 없으면 만드는 법을 적어 죽는다(무음 금지).
pub fn load(stem: &str) -> ScreenState {
    let path = path_of(stem);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "골든이 없다: {} ({e})\n\
             부트스트랩: 유닛에서 `SOKSAK_GOLDEN_OUT=<dir> cargo test --test conformance -- --ignored dump_goldens`\n\
             — 그 산출물은 후보일 뿐이다. 엔진끼리 대조하고 SPEC.md §11·§12 로 판정한 뒤에만 골든이 된다.",
            path.display()
        )
    });
    from_text(&text).unwrap_or_else(|e| panic!("골든이 깨졌다 {}: {e}", path.display()))
}

/// 골든 파일 경로 — 이 크레이트의 `goldens/` 아래. 유닛이 path 의존으로 물어도 계약 repo 를 가리킨다.
pub fn path_of(stem: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("goldens").join(format!("{stem}.golden"))
}

// ── 직렬화 ───────────────────────────────────────────────────────────────────

pub fn to_text(s: &ScreenState) -> String {
    let mut out = String::new();
    out.push_str("# soksak-contract-terminal — 선언된 화면 상태(골든).\n");
    out.push_str("# 포맷: SPEC.md §12. 정규화 규칙(무엇을 같다고 보는가): SPEC.md §11.\n");
    out.push_str(&format!("cols {}\n", s.cols));
    out.push_str(&format!("rows {}\n", s.rows));
    out.push_str(&format!("alt {}\n", b(s.alt)));
    out.push_str(&format!("cursor {} {}\n", s.cursor.0, s.cursor.1));
    let flags = s.modes.to_flags();
    out.push_str("modes");
    for f in flags {
        out.push_str(&format!(" {}", b(f)));
    }
    out.push_str("   # ");
    out.push_str(&Modes::NAMES.join(" "));
    out.push('\n');
    out.push_str(&format!("history {}\n", s.history.len()));
    out.push_str(&format!("visible {}\n", s.visible.len()));

    for (i, row) in s.history.iter().enumerate() {
        push_row(&mut out, &format!("H{i:04}"), row);
    }
    for (i, row) in s.visible.iter().enumerate() {
        push_row(&mut out, &format!("V{i:02}"), row);
    }
    out
}

fn push_row(out: &mut String, label: &str, row: &Row) {
    out.push_str(&format!("# |{}|\n", row.text()));
    out.push_str(label);
    // 같은 칸이 이어지면 접는다 — 넓은 빈 구간이 한 토큰이 된다.
    let mut i = 0;
    while i < row.0.len() {
        let cell = &row.0[i];
        let mut run = 1;
        while i + run < row.0.len() && row.0[i + run] == *cell {
            run += 1;
        }
        out.push(' ');
        if run > 1 {
            out.push_str(&format!("{run}*"));
        }
        out.push_str(&cell_token(cell));
        i += run;
    }
    out.push('\n');
}

fn cell_token(c: &Cell) -> String {
    if c.is_blank_default() {
        return ".".to_string();
    }
    let cp: Vec<String> = c.text.chars().map(|ch| format!("{:X}", ch as u32)).collect();
    format!(
        "{}:{}:{}:{}:{}",
        cp.join("."),
        if c.wide { "w" } else { "n" },
        color_token(c.fg),
        color_token(c.bg),
        attrs_token(&c.attrs)
    )
}

fn color_token(c: Color) -> String {
    match c {
        Color::Default => "-".to_string(),
        Color::Palette(i) => format!("p{i}"),
        Color::Rgb(r, g, bl) => format!("{r:02X}{g:02X}{bl:02X}"),
    }
}

fn attrs_token(a: &Attrs) -> String {
    if a.is_default() {
        return "-".to_string();
    }
    let mut s = String::new();
    if a.bold {
        s.push('b');
    }
    if a.dim {
        s.push('d');
    }
    if a.italic {
        s.push('i');
    }
    if a.underline {
        s.push('u');
    }
    if a.inverse {
        s.push('v');
    }
    if a.strikeout {
        s.push('s');
    }
    if a.hidden {
        s.push('h');
    }
    s
}

fn b(v: bool) -> u8 {
    if v {
        1
    } else {
        0
    }
}

// ── 역직렬화 ─────────────────────────────────────────────────────────────────

pub fn from_text(text: &str) -> Result<ScreenState, String> {
    let mut cols = 0u16;
    let mut rows = 0u16;
    let mut alt = false;
    let mut cursor = (0u16, 0u16);
    let mut modes = Modes::default();
    let mut history: Vec<Row> = Vec::new();
    let mut visible: Vec<Row> = Vec::new();

    for (n, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let mut it = line.split_whitespace();
        let key = it.next().ok_or_else(|| format!("{}: 빈 줄", n + 1))?;
        let err = |what: &str| format!("{}행: {what}", n + 1);
        match key {
            "cols" => cols = parse(it.next(), &err("cols"))?,
            "rows" => rows = parse(it.next(), &err("rows"))?,
            "alt" => alt = parse::<u8>(it.next(), &err("alt"))? != 0,
            "cursor" => {
                cursor.0 = parse(it.next(), &err("cursor x"))?;
                cursor.1 = parse(it.next(), &err("cursor y"))?;
            }
            "modes" => {
                let mut flags = [false; 13];
                for (i, slot) in flags.iter_mut().enumerate() {
                    *slot = parse::<u8>(it.next(), &err(&format!("mode {i}")))? != 0;
                }
                modes = Modes::from_flags(flags);
            }
            // 행 수는 행 줄이 진실이다 — 선언은 사람이 읽는 요약이라 굳이 대조하지 않는다.
            "history" | "visible" => {}
            _ if key.starts_with('H') || key.starts_with('V') => {
                let row = parse_row(&line[key.len()..]).map_err(|e| err(&e))?;
                if key.starts_with('H') {
                    history.push(row);
                } else {
                    visible.push(row);
                }
            }
            other => return Err(err(&format!("모르는 키 {other}"))),
        }
    }

    Ok(ScreenState { cols, rows, alt, cursor, modes, history, visible })
}

fn parse<T: std::str::FromStr>(tok: Option<&str>, what: &str) -> Result<T, String> {
    tok.ok_or_else(|| format!("{what}: 값 없음"))?
        .parse()
        .map_err(|_| format!("{what}: 못 읽는 값"))
}

fn parse_row(rest: &str) -> Result<Row, String> {
    let mut cells: Vec<Cell> = Vec::new();
    for tok in rest.split_whitespace() {
        let (run, body) = match tok.split_once('*') {
            Some((n, rest)) => (n.parse::<usize>().map_err(|_| format!("반복 수 {n}"))?, rest),
            None => (1, tok),
        };
        let cell = parse_cell(body)?;
        for _ in 0..run {
            cells.push(cell.clone());
        }
    }
    Ok(Row(cells))
}

fn parse_cell(tok: &str) -> Result<Cell, String> {
    if tok == "." {
        return Ok(Cell::blank());
    }
    let f: Vec<&str> = tok.split(':').collect();
    if f.len() != 5 {
        return Err(format!("칸 토큰 {tok}: 다섯 칸이 아니다"));
    }
    let mut text = String::new();
    for cp in f[0].split('.') {
        let v = u32::from_str_radix(cp, 16).map_err(|_| format!("코드포인트 {cp}"))?;
        text.push(char::from_u32(v).ok_or_else(|| format!("코드포인트 {cp}"))?);
    }
    let wide = match f[1] {
        "n" => false,
        "w" => true,
        w => return Err(format!("폭 {w}")),
    };
    Ok(Cell { text, fg: parse_color(f[2])?, bg: parse_color(f[3])?, attrs: parse_attrs(f[4])?, wide })
}

fn parse_color(tok: &str) -> Result<Color, String> {
    if tok == "-" {
        return Ok(Color::Default);
    }
    if let Some(idx) = tok.strip_prefix('p') {
        return Ok(Color::Palette(idx.parse().map_err(|_| format!("팔레트 {tok}"))?));
    }
    if tok.len() == 6 {
        let v = |i: usize| u8::from_str_radix(&tok[i..i + 2], 16).map_err(|_| format!("색 {tok}"));
        return Ok(Color::Rgb(v(0)?, v(2)?, v(4)?));
    }
    Err(format!("색 {tok}"))
}

fn parse_attrs(tok: &str) -> Result<Attrs, String> {
    let mut a = Attrs::default();
    if tok == "-" {
        return Ok(a);
    }
    for c in tok.chars() {
        match c {
            'b' => a.bold = true,
            'd' => a.dim = true,
            'i' => a.italic = true,
            'u' => a.underline = true,
            'v' => a.inverse = true,
            's' => a.strikeout = true,
            'h' => a.hidden = true,
            other => return Err(format!("속성 {other}")),
        }
    }
    Ok(a)
}

#[cfg(test)]
mod tests {
    use super::*;

    // 코덱은 왕복해야 한다 — 골든이 읽고 쓰는 사이에 상태가 새면 시험 전체가 무의미하다.
    #[test]
    fn round_trips_a_state() {
        let s = ScreenState {
            cols: 80,
            rows: 2,
            alt: true,
            cursor: (3, 1),
            modes: Modes { bracketed_paste: true, line_wrap: true, ..Modes::default() },
            history: vec![Row::normalized(vec![Cell {
                text: "가".into(),
                fg: Color::Rgb(1, 2, 3),
                bg: Color::Palette(9),
                attrs: Attrs { bold: true, underline: true, ..Attrs::default() },
                wide: true,
            }])],
            visible: vec![
                Row::normalized(vec![Cell { text: "A".into(), ..Cell::blank() }, Cell::blank()]),
                Row::default(),
            ],
        };
        let text = to_text(&s);
        assert_eq!(from_text(&text).expect("parse"), s, "골든 코덱은 왕복해야 한다");
    }
}
