//! 화면 상태의 **정규형** — 계약이 선언하는 비교 통화. 근거와 정규화 규칙은 SPEC.md §11 이
//! 소유하며, 이 파일은 그 선언을 타입으로 옮긴 것이다.
//!
//! 엔진마다 정당하게 다른 표현(팔레트 색을 이름으로 접는다든지, wide 문자의 스페이서를 어떻게
//! 담는다든지)은 여기서 **하나로 접힌다**. 접는 방식은 계약이 판정했다 — 지금까지 그 판정을 특정
//! 엔진이 암묵 대행했고, 이제 명문이다.

/// 셀 색. 팔레트 인덱스는 인덱스 그대로다 — 0..16 을 "이름 있는 색"으로 따로 접지 않는다(그 구분은
/// 한 엔진의 내부 표현이지 화면의 성질이 아니다). 기본색은 테마 상대값이라 RGB 로 해소하지 않는다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    Default,
    Palette(u8),
    Rgb(u8, u8, u8),
}

/// 셀 속성. underline 은 밑줄 **모양**(single/double/curly…)을 구분하지 않는다 — 계약이 복원하는
/// 것은 SGR 4 하나이고, 모양은 이 계약의 화면 동치에 들지 않는다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Attrs {
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub inverse: bool,
    pub strikeout: bool,
    pub hidden: bool,
}

impl Attrs {
    pub fn is_default(&self) -> bool {
        *self == Attrs::default()
    }
}

/// 한 칸. wide 문자는 `wide = true` 인 **본체 한 칸**으로만 담는다 — 뒤따르는 점유 스페이서는
/// 표현하지 않는다(본체가 두 칸을 먹는다는 사실에서 유도된다). 텍스트 없는 칸은 공백 한 칸이다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    /// 이 칸의 그래핌 클러스터(본체 코드포인트 + 결합 문자).
    pub text: String,
    pub fg: Color,
    pub bg: Color,
    pub attrs: Attrs,
    /// 두 칸을 먹는 문자인가.
    pub wide: bool,
}

impl Cell {
    pub fn blank() -> Self {
        Cell {
            text: " ".to_string(),
            fg: Color::Default,
            bg: Color::Default,
            attrs: Attrs::default(),
            wide: false,
        }
    }

    /// 공백 칸의 정규화 — **공백에는 글리프가 없다**. 그래서 fg·bold·dim·italic·hidden 은 공백에
    /// 아무것도 그리지 않는다(보이는 것은 bg·inverse·underline·strikeout 뿐이다). 화면 동치는 사람이
    /// 보는 것을 기준으로 하므로, 공백에 얹힌 그 보이지 않는 속성은 정규형에서 떨어뜨린다.
    ///
    /// 이 규칙이 필요한 이유는 실측이다: 어떤 엔진은 자동 줄바꿈으로 새 줄을 만들 때 손대지 않은 칸에
    /// 그때의 펜(SGR)을 물려준다 — 그 칸들은 화면에 공백으로 보이지만 모델에는 색과 굵기가 남는다.
    /// 그 차이는 화면이 아니라 표현의 차이이므로 계약이 접는다.
    ///
    /// inverse 는 예외다: 반전이 걸린 공백은 fg 가 **배경으로 드러나** 보이므로 fg 를 남긴다.
    pub fn canonical(mut self) -> Self {
        if self.text == " " && !self.attrs.inverse {
            self.fg = Color::Default;
            self.attrs.bold = false;
            self.attrs.dim = false;
            self.attrs.italic = false;
            self.attrs.hidden = false;
        }
        self
    }

    /// 스타일 없는 빈 칸 — 행 꼬리 절단의 기준(정규화 뒤의 판정이다).
    pub fn is_blank_default(&self) -> bool {
        self.text == " "
            && self.fg == Color::Default
            && self.bg == Color::Default
            && self.attrs.is_default()
            && !self.wide
    }
}

/// 한 행. 꼬리의 빈 기본 칸은 **잘라 낸다** — 터미널이 행 끝까지 공백을 채웠든 손대지 않았든 사람
/// 눈에는 같은 화면이므로, 그 차이를 화면 동치에 넣지 않는다.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Row(pub Vec<Cell>);

impl Row {
    /// 칸마다 [`Cell::canonical`] 을 적용하고 꼬리의 빈 칸을 잘라 정규화한다.
    pub fn normalized(cells: Vec<Cell>) -> Self {
        let mut cells: Vec<Cell> = cells.into_iter().map(Cell::canonical).collect();
        while cells.last().map_or(false, |c| c.is_blank_default()) {
            cells.pop();
        }
        Row(cells)
    }

    /// 스타일 무시한 행 텍스트(사람이 읽는 면).
    pub fn text(&self) -> String {
        self.0.iter().map(|c| c.text.as_str()).collect()
    }
}

/// 복원 대상 private mode 집합 — rehydrate 가 재현해야 하는 전부. 순서는 골든 직렬화가 쓰는
/// 순서이기도 하다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Modes {
    pub bracketed_paste: bool,
    pub app_cursor: bool,
    pub app_keypad: bool,
    pub mouse_click: bool,
    pub mouse_drag: bool,
    pub mouse_motion: bool,
    pub sgr_mouse: bool,
    pub utf8_mouse: bool,
    pub focus_in_out: bool,
    pub alternate_scroll: bool,
    pub show_cursor: bool,
    pub line_wrap: bool,
    pub insert: bool,
}

impl Modes {
    /// 골든 직렬화 순서와 같은 13칸 배열.
    pub fn to_flags(self) -> [bool; 13] {
        [
            self.bracketed_paste,
            self.app_cursor,
            self.app_keypad,
            self.mouse_click,
            self.mouse_drag,
            self.mouse_motion,
            self.sgr_mouse,
            self.utf8_mouse,
            self.focus_in_out,
            self.alternate_scroll,
            self.show_cursor,
            self.line_wrap,
            self.insert,
        ]
    }

    pub fn from_flags(f: [bool; 13]) -> Self {
        Modes {
            bracketed_paste: f[0],
            app_cursor: f[1],
            app_keypad: f[2],
            mouse_click: f[3],
            mouse_drag: f[4],
            mouse_motion: f[5],
            sgr_mouse: f[6],
            utf8_mouse: f[7],
            focus_in_out: f[8],
            alternate_scroll: f[9],
            show_cursor: f[10],
            line_wrap: f[11],
            insert: f[12],
        }
    }

    pub const NAMES: [&'static str; 13] = [
        "bracketed_paste",
        "app_cursor",
        "app_keypad",
        "mouse_click",
        "mouse_drag",
        "mouse_motion",
        "sgr_mouse",
        "utf8_mouse",
        "focus_in_out",
        "alternate_scroll",
        "show_cursor",
        "line_wrap",
        "insert",
    ];
}

/// 화면 상태 정규형 — 계약이 "이 스트림을 먹으면 화면은 이래야 한다"고 선언하는 대상.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenState {
    pub cols: u16,
    pub rows: u16,
    /// alt-screen 이 활성인가.
    pub alt: bool,
    /// 커서 (x, y) — 활성 영역 기준 0-base. 커서 표시 여부는 [`Modes::show_cursor`](DECTCEM)가
    /// 이미 담는다 — 같은 사실을 정규형에 두 벌 두지 않는다.
    pub cursor: (u16, u16),
    pub modes: Modes,
    /// 스크롤백 — 가장 오래된 행부터.
    pub history: Vec<Row>,
    /// 보이는 화면 — 위에서 아래로, 길이 = rows.
    pub visible: Vec<Row>,
}
