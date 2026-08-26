//! The `frame` wire (SPEC.md §5.1): the reply shape, the reference `apply`, and the assertions
//! that grade a reply series against a declared reference state.
//!
//! A series is what one subscriber receives when a fixture stream is fed in three cuts at 80×24
//! and `frame` is requested after each cut at offset 0. The first reply is full; the rest carry
//! changed rows only. Folding the series with `apply` must reproduce the declared screen — the
//! same reference state that grades interpretation (§9), read through the wire instead of through
//! `ScreenState`. The wire cannot say something the canonical form does not.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::corpus::{COLS, Fixture, ROWS};
use crate::reference_state;
use crate::state::{Attrs, Color, Modes, Row, ScreenState};

/// Cuts per series: the stream is fed in thirds and a reply is taken after each.
pub const CUT_POINTS: usize = 3;

/// `attrs` bits on the wire.
pub const ATTR_BOLD: u16 = 1;
pub const ATTR_DIM: u16 = 2;
pub const ATTR_ITALIC: u16 = 4;
pub const ATTR_UNDERLINE: u16 = 8;
pub const ATTR_INVERSE: u16 = 16;
pub const ATTR_STRIKEOUT: u16 = 32;
pub const ATTR_HIDDEN: u16 = 64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameRun {
    pub text: String,
    pub fg: String,
    pub bg: String,
    pub attrs: u16,
    pub n: u32,
    #[serde(default, skip_serializing_if = "is_false")]
    pub wide: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameLine {
    pub y: u16,
    pub wrapped: bool,
    pub runs: Vec<FrameRun>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameReply {
    pub output_sequence: u64,
    pub cols: u16,
    pub rows: u16,
    /// `[row, col]`, 0-based.
    pub cursor: (u16, u16),
    pub cursor_visible: bool,
    pub alt_active: bool,
    pub history_size: usize,
    pub offset: usize,
    pub modes: Modes,
    pub full: bool,
    pub lines: Vec<FrameLine>,
}

/// One fixture's declared reply series.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameSeries {
    pub fixture: String,
    pub cols: u16,
    pub rows: u16,
    pub cuts: Vec<usize>,
    pub replies: Vec<FrameReply>,
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// Byte offsets at which a stream of `len` bytes is cut; the last cut is the whole stream.
pub fn cut_points(len: usize) -> [usize; CUT_POINTS] {
    [len / 3, len * 2 / 3, len]
}

/// The contract-owned series path: `reference_states/frames/<stem>.frames.json`.
pub fn series_path(stem: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("reference_states")
        .join("frames")
        .join(format!("{stem}.frames.json"))
}

/// Reads the declared series. Missing or invalid is an explicit failure.
pub fn load_series(stem: &str) -> FrameSeries {
    let path = series_path(stem);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "frame series is missing: {} ({e})\n\
             bootstrap from a unit: `SOKSAK_FRAME_SERIES_OUT=<dir> cargo test --release --test conformance -- --ignored dump_frame_series`\n\
             The output stands only because the fold reproduces the declared reference state (SPEC.md §12).",
            path.display()
        )
    });
    FrameSeries::from_json(&text)
        .unwrap_or_else(|e| panic!("invalid frame series {}: {e}", path.display()))
}

impl FrameSeries {
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("frame series serializes")
    }

    pub fn from_json(text: &str) -> Result<Self, String> {
        serde_json::from_str(text).map_err(|e| e.to_string())
    }
}

/// The reference fold. A full reply replaces the picture. A delta replaces every header field and
/// the rows it names; rows it does not name stay as they were. The result is always full.
pub fn apply(previous: &FrameReply, reply: &FrameReply) -> FrameReply {
    let mut applied = reply.clone();
    applied.full = true;
    if reply.full {
        return applied;
    }
    applied.lines = previous.lines.clone();
    for line in &reply.lines {
        match applied.lines.iter_mut().find(|slot| slot.y == line.y) {
            Some(slot) => *slot = line.clone(),
            None => applied.lines.push(line.clone()),
        }
    }
    applied.lines.sort_by_key(|line| line.y);
    applied
}

/// Folds a series in order. The first reply must be full.
pub fn fold(replies: &[FrameReply]) -> FrameReply {
    let (first, rest) = replies
        .split_first()
        .expect("a series has at least one reply");
    assert!(first.full, "the first reply of a series is full");
    let mut state = first.clone();
    state.full = true;
    for reply in rest {
        state = apply(&state, reply);
    }
    state
}

pub fn color_string(color: Color) -> String {
    match color {
        Color::Default => "default".to_string(),
        Color::Palette(i) => format!("palette:{i}"),
        Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
    }
}

pub fn attr_bits(attrs: &Attrs) -> u16 {
    (attrs.bold as u16) * ATTR_BOLD
        | (attrs.dim as u16) * ATTR_DIM
        | (attrs.italic as u16) * ATTR_ITALIC
        | (attrs.underline as u16) * ATTR_UNDERLINE
        | (attrs.inverse as u16) * ATTR_INVERSE
        | (attrs.strikeout as u16) * ATTR_STRIKEOUT
        | (attrs.hidden as u16) * ATTR_HIDDEN
}

/// A canonical row as runs: maximal adjacent cells with equal `(fg, bg, attrs, wide)`, `n` counting
/// two per wide glyph. The canonical row already dropped spacers, normalized blanks and trimmed
/// its tail, which is why the wire's run rule and this one meet.
pub fn runs_of(row: &Row) -> Vec<FrameRun> {
    let mut runs: Vec<FrameRun> = Vec::new();
    for cell in &row.0 {
        let fg = color_string(cell.fg);
        let bg = color_string(cell.bg);
        let attrs = attr_bits(&cell.attrs);
        let width = if cell.wide { 2 } else { 1 };
        match runs.last_mut() {
            Some(last)
                if last.fg == fg && last.bg == bg && last.attrs == attrs && last.wide == cell.wide =>
            {
                last.text.push_str(&cell.text);
                last.n += width;
            }
            _ => runs.push(FrameRun {
                text: cell.text.clone(),
                fg,
                bg,
                attrs,
                n: width,
                wide: cell.wide,
                link: None,
            }),
        }
    }
    runs
}

fn run_text(runs: &[FrameRun]) -> String {
    runs.iter().map(|run| run.text.as_str()).collect()
}

/// Grades a full reply against a declared screen. Points at the first row that differs.
pub fn assert_frame_matches(reply: &FrameReply, expected: &ScreenState, ctx: &str) {
    assert!(reply.full, "{ctx}: a graded reply is full");
    assert_eq!(reply.cols, expected.cols, "{ctx}: cols");
    assert_eq!(reply.rows, expected.rows, "{ctx}: rows");
    assert_eq!(reply.alt_active, expected.alt, "{ctx}: altActive");
    assert_eq!(
        reply.cursor,
        (expected.cursor.1, expected.cursor.0),
        "{ctx}: cursor [row, col]"
    );
    assert_eq!(reply.modes, expected.modes, "{ctx}: modes");
    assert_eq!(
        reply.cursor_visible, expected.modes.show_cursor,
        "{ctx}: cursorVisible follows modes.showCursor"
    );
    assert_eq!(
        reply.history_size,
        expected.history.len(),
        "{ctx}: historySize"
    );
    assert_eq!(
        reply.lines.len(),
        expected.visible.len(),
        "{ctx}: one line per visible row"
    );
    for (y, (line, row)) in reply.lines.iter().zip(&expected.visible).enumerate() {
        assert_eq!(usize::from(line.y), y, "{ctx}: line {y} carries its own y");
        let want = runs_of(row);
        if line.runs == want {
            continue;
        }
        assert_eq!(
            run_text(&line.runs),
            row.text(),
            "{ctx}: V{y:02} text"
        );
        assert_eq!(line.runs, want, "{ctx}: V{y:02} runs");
    }
}

/// Grades a whole series: shape, cut points, sequence, and the fold against the reference state.
pub fn assert_series_reproduces(series: &FrameSeries, fixture: Fixture) {
    let stem = fixture.stem();
    assert_eq!(series.fixture, stem, "{stem}: series names another fixture");
    assert_eq!((series.cols, series.rows), (COLS, ROWS), "{stem}: series grid");
    assert_eq!(
        series.cuts,
        cut_points(fixture.stream().len()).to_vec(),
        "{stem}: cut points follow the corpus length"
    );
    assert_eq!(series.replies.len(), CUT_POINTS, "{stem}: one reply per cut");
    assert!(series.replies[0].full, "{stem}: the first reply is full");
    for (index, (reply, cut)) in series.replies.iter().zip(&series.cuts).enumerate() {
        assert_eq!(
            reply.output_sequence, *cut as u64,
            "{stem}: reply {index} outputSequence is its cut"
        );
        assert_eq!((reply.cols, reply.rows), (COLS, ROWS), "{stem}: reply {index} grid");
        assert_eq!(reply.offset, 0, "{stem}: reply {index} is taken at offset 0");
    }
    let folded = fold(&series.replies);
    assert_frame_matches(&folded, &reference_state::load(stem), stem);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Cell;

    fn line(y: u16, text: &str) -> FrameLine {
        FrameLine {
            y,
            wrapped: false,
            runs: vec![FrameRun {
                text: text.into(),
                fg: "default".into(),
                bg: "default".into(),
                attrs: 0,
                n: text.chars().count() as u32,
                wide: false,
                link: None,
            }],
        }
    }

    fn reply(full: bool, lines: Vec<FrameLine>) -> FrameReply {
        FrameReply {
            output_sequence: 1,
            cols: 4,
            rows: 2,
            cursor: (0, 0),
            cursor_visible: true,
            alt_active: false,
            history_size: 0,
            offset: 0,
            modes: Modes::default(),
            full,
            lines,
        }
    }

    #[test]
    fn apply_replaces_only_the_rows_a_delta_carries() {
        let first = reply(true, vec![line(0, "a"), line(1, "b")]);
        let mut delta = reply(false, vec![line(1, "c")]);
        delta.output_sequence = 2;
        let applied = apply(&first, &delta);
        assert!(applied.full);
        assert_eq!(applied.output_sequence, 2);
        assert_eq!(applied.lines, vec![line(0, "a"), line(1, "c")]);
        assert_eq!(apply(&applied, &first).lines, first.lines);
    }

    #[test]
    fn runs_of_merges_equal_style_cells_and_counts_wide_as_two() {
        let bold = Attrs {
            bold: true,
            ..Attrs::default()
        };
        let row = Row::normalized(vec![
            Cell {
                text: "a".into(),
                attrs: bold,
                ..Cell::blank()
            },
            Cell {
                text: "b".into(),
                attrs: bold,
                ..Cell::blank()
            },
            Cell {
                text: "가".into(),
                wide: true,
                ..Cell::blank()
            },
            Cell::blank(),
        ]);
        let runs = runs_of(&row);
        assert_eq!(runs.len(), 2);
        assert_eq!((runs[0].text.as_str(), runs[0].n, runs[0].attrs), ("ab", 2, ATTR_BOLD));
        assert_eq!((runs[1].text.as_str(), runs[1].n, runs[1].wide), ("가", 2, true));
    }
}
