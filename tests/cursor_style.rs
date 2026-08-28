use soksak_contract_terminal::{
    CursorShape, CursorStyle, MirrorUnderTest, Modes, Row, ScreenState,
    assert_cursor_style_conforms,
};

struct ScriptedCursor {
    cols: u16,
    rows: u16,
    style: CursorStyle,
    modes: Modes,
}

impl MirrorUnderTest for ScriptedCursor {
    fn new(cols: u16, rows: u16) -> Self {
        Self {
            cols,
            rows,
            style: CursorStyle {
                shape: CursorShape::Block,
                blinking: true,
            },
            modes: Modes {
                show_cursor: true,
                line_wrap: true,
                ..Modes::default()
            },
        }
    }

    fn feed(&mut self, bytes: &[u8]) {
        self.style = match bytes {
            b"\x1b[1 q" => CursorStyle { shape: CursorShape::Block, blinking: true },
            b"\x1b[2 q" => CursorStyle { shape: CursorShape::Block, blinking: false },
            b"\x1b[3 q" => CursorStyle { shape: CursorShape::Underline, blinking: true },
            b"\x1b[4 q" => CursorStyle { shape: CursorShape::Underline, blinking: false },
            b"\x1b[5 q" => CursorStyle { shape: CursorShape::Bar, blinking: true },
            b"\x1b[6 q" => CursorStyle { shape: CursorShape::Bar, blinking: false },
            b"\x1b[?12h" => CursorStyle { blinking: true, ..self.style },
            b"\x1b[?12l" => CursorStyle { blinking: false, ..self.style },
            b"\x1b[?25h" => { self.modes.show_cursor = true; self.style },
            b"\x1b[?25l" => { self.modes.show_cursor = false; self.style },
            _ => self.style,
        };
    }

    fn resize(&mut self, cols: u16, rows: u16) {
        self.cols = cols;
        self.rows = rows;
    }

    fn rehydrate(&self) -> Vec<u8> {
        match self.style {
            CursorStyle { shape: CursorShape::Block, blinking: true } => b"\x1b[1 q".to_vec(),
            CursorStyle { shape: CursorShape::Block, blinking: false } => b"\x1b[2 q".to_vec(),
            CursorStyle { shape: CursorShape::Underline, blinking: true } => b"\x1b[3 q".to_vec(),
            CursorStyle { shape: CursorShape::Underline, blinking: false } => b"\x1b[4 q".to_vec(),
            CursorStyle { shape: CursorShape::Bar, blinking: true } => b"\x1b[5 q".to_vec(),
            CursorStyle { shape: CursorShape::Bar, blinking: false } => b"\x1b[6 q".to_vec(),
        }
    }
    fn cold_paint(&self) -> Vec<u8> { Vec::new() }
    fn suppressed_replies(&self) -> u64 { 0 }
    fn cursor_style(&self) -> CursorStyle { self.style }

    fn screen_state(&self) -> ScreenState {
        ScreenState {
            cols: self.cols,
            rows: self.rows,
            alt: false,
            cursor: (0, 0),
            modes: self.modes,
            history: Vec::new(),
            visible: vec![Row::default(); self.rows as usize],
        }
    }
}

#[test]
fn dec_cursor_style_and_visibility_are_separate_engine_state() {
    assert_cursor_style_conforms::<ScriptedCursor>();
}
