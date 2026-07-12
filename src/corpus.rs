//! 코퍼스 — 합격시험 일곱 건의 입력 바이트. 계약이 소유한다(유닛에 사본 없음).
//!
//! 스트림은 실물 조건을 노린다: 데몬 세션 링 용량([`RING`])과 같은 지점에 절단을 놓아, 링이
//! 이스케이프 한가운데·UTF-8 한가운데를 자르게 만든다. 그래서 바이트 길이가 픽스처의 전제다 —
//! 길이를 바꾸면 절단 지점이 어긋나 시험이 겨누던 결함을 놓친다.

/// 미러 격자 — 계약이 고정한다(골든이 이 격자를 전제로 선언돼 있다).
pub const COLS: u16 = 80;
pub const ROWS: u16 = 24;

/// 픽스처가 넘겨야 하는 창의 크기(1 MiB) — 스트림이 이만큼을 넘겨야 "창 밖으로 밀린 상태"가
/// 실물 조건이 된다.
///
/// **주석이 사실과 어긋나 있었다.** 이 값을 "데몬 세션 링 용량과 같은 값"이라고 적어 두었으나,
/// 데몬(soksak-ptyd)의 실제 값은 그것이 아니다: 원시 링은 `RING_CAP` = 256 KiB 이고, tee 구독자
/// 버퍼는 `TEE_BUF_CAP` = 1,000,000 바이트다. 1 MiB 는 그 둘 **모두를 넉넉히 넘긴다** — 픽스처가
/// 노리는 조건(모드 세트가 창 밖으로 밀린다·스크롤백 창이 넘친다)은 그대로 성립하므로 값은 바꾸지
/// 않는다. 바꾸면 일곱 스트림의 바이트 길이가 전부 달라지고 골든 전체가 흔들리는데, 그 대가로
/// 얻는 것이 없다.
///
/// 또 하나 — 이 상수로 맞춰 둔 **이스케이프 중간 절단 정렬은 합격시험이 쓰지 않는다**. 시험은
/// 미러에 스트림을 통째로 먹인다(SPEC.md §9). 그 정렬이 겨누는 것은 데몬 쪽 절단이므로, 그것을
/// 채점하려면 실 데몬 통합에서 링 경계를 태워야 한다(유닛의 ptyd 통합 시험이 그 자리다).
pub const RING: usize = 1_048_576;

/// 합격시험 일곱 건.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fixture {
    /// ① 링 절단이 이스케이프 시퀀스 한가운데 떨어진다.
    MidEscapeTail,
    /// ② UTF-8 문자 중간 절단 + wide 문자 열 정렬.
    CjkWidth,
    /// ③ alt-screen 진입/이탈 — TUI 화면과 그 뒤 프라임 스크롤백.
    AltScreen,
    /// ④ private mode 가 링 창 밖에서도 산다.
    PrivateModes,
    /// ⑤ 재생분 안의 질의에 어떤 경로도 다시 응답하지 않는다.
    ReplayGuard,
    /// ⑥ cold 체크포인트가 alt-screen(TUI) 프레임을 텍스트로 담는다.
    ColdPaintAlt,
    /// ⑦ DEC Special Graphics 번역이 그리드에 남긴 박스 글리프.
    DecLineDrawing,
}

impl Fixture {
    /// 일곱 건 전부(테스트가 훑는 순서).
    pub const ALL: [Fixture; 7] = [
        Fixture::MidEscapeTail,
        Fixture::CjkWidth,
        Fixture::AltScreen,
        Fixture::PrivateModes,
        Fixture::ReplayGuard,
        Fixture::ColdPaintAlt,
        Fixture::DecLineDrawing,
    ];

    /// 골든 파일 이름의 어간(goldens/<stem>.golden).
    pub fn stem(self) -> &'static str {
        match self {
            Fixture::MidEscapeTail => "mid_escape_tail",
            Fixture::CjkWidth => "cjk_width",
            Fixture::AltScreen => "alt_screen",
            Fixture::PrivateModes => "private_modes",
            Fixture::ReplayGuard => "replay_guard",
            Fixture::ColdPaintAlt => "cold_paint_alt",
            Fixture::DecLineDrawing => "dec_line_drawing",
        }
    }

    /// 세션 출력 바이트 — 미러가 먹는 스트림.
    pub fn stream(self) -> Vec<u8> {
        match self {
            Fixture::MidEscapeTail => mid_escape_tail(),
            Fixture::CjkWidth => cjk_width(),
            Fixture::AltScreen => alt_screen(),
            Fixture::PrivateModes => private_modes(),
            Fixture::ReplayGuard => replay_guard(),
            Fixture::ColdPaintAlt => cold_paint_alt(),
            Fixture::DecLineDrawing => dec_line_drawing(),
        }
    }

    /// ③ 은 이탈 국면이 더 있다 — alt 를 벗고 나서의 화면도 계약이 규정한다.
    pub fn epilogue(self) -> Option<Vec<u8>> {
        match self {
            Fixture::AltScreen => Some(b"\x1b[?25h\x1b[?1049lBACK-MARK\r\n".to_vec()),
            _ => None,
        }
    }
}

// ── 행 생성기 ────────────────────────────────────────────────────────────────

/// 트루컬러 wide 문자 행(80칸·바이트 길이 고정 1046) — 행마다 색이 달라 정렬 오차가 보인다.
/// 행의 첫 바이트가 이스케이프 시작(\x1b)이라 절단 지점을 이스케이프 중간에 놓을 수 있다.
/// 행이 1KB 를 넘어야 링(1MB)이 스크롤백 창(1000행)보다 적은 행을 담는다 — 절단 손상이
/// 비교 창 안에 남는 실물 조건(짙은 화면일수록 링이 담는 시간이 짧다)이다.
pub fn heavy_row(i: usize) -> Vec<u8> {
    let mut v = Vec::with_capacity(1046);
    for j in 0..40 {
        let r = 100 + ((i * 7 + j * 13) % 156);
        let g = 100 + ((i * 11 + j * 3) % 156);
        let b = 100 + ((i * 5 + j * 17) % 156);
        v.extend_from_slice(format!("\x1b[0;1;38;2;{r};{g};{b}m가").as_bytes());
    }
    v.extend_from_slice(b"\x1b[0m\r\n");
    assert_eq!(v.len(), 1046);
    v
}

/// 마지막을 정확한 바이트 수로 맞추는 패딩 줄("PAD:xxx…\r\n").
fn pad_line(len: usize) -> Vec<u8> {
    assert!(len >= 7, "pad too small: {len}");
    let mut v = b"PAD:".to_vec();
    v.extend(std::iter::repeat(b'x').take(len - 6));
    v.extend_from_slice(b"\r\n");
    v
}

// ── ① mid-escape tail — 링 절단이 이스케이프 시퀀스 한가운데 떨어진다 ─────────

fn mid_escape_tail() -> Vec<u8> {
    // rows 블록을 정확히 RING+3 바이트로 만들면, 링(마지막 RING 바이트)은 rows[0] 의
    // "\x1b[3…" 에서 3바이트 들어간 "8;2;…" 부터 시작한다 — 이스케이프 중간 절단.
    let mut rows: Vec<u8> = Vec::new();
    let mut i = 0;
    while rows.len() < RING + 3 - 2000 {
        rows.extend(heavy_row(i));
        i += 1;
    }
    rows.extend_from_slice(b"\x1b[31mTAIL-END-MARK\x1b[0m\r\n");
    let deficit = RING + 3 - rows.len();
    rows.extend(pad_line(deficit));
    assert_eq!(rows.len(), RING + 3, "절단 정렬이 픽스처의 전제");

    let mut stream = b"SEED-MARK\r\n".to_vec();
    stream.extend_from_slice(&rows);
    stream
}

// ── ② CJK/unicode 폭 — UTF-8 문자 중간 절단 + wide 문자 열 정렬 ───────────────

fn cjk_width() -> Vec<u8> {
    // wide 문자 행(첫 문자 '한' = 3바이트). rows 블록 == RING+1 이면 링은 '한'의
    // 두 번째 바이트부터 시작한다 — UTF-8 중간 절단.
    fn cjk_row(i: usize) -> Vec<u8> {
        let mut v = b"\xed\x95\x9c".to_vec(); // '한'
        for j in 0..39 {
            let r = 100 + ((i * 3 + j * 19) % 156);
            let g = 100 + ((i * 13 + j * 7) % 156);
            let b = 100 + ((i * 17 + j * 5) % 156);
            v.extend_from_slice(format!("\x1b[0;1;7;38;2;{r};{g};{b}m나").as_bytes());
        }
        v.extend_from_slice(b"\x1b[0m\r\n");
        assert_eq!(v.len(), 1101);
        v
    }

    let mut rows: Vec<u8> = Vec::new();
    let mut i = 0;
    while rows.len() < RING + 1 - 2000 {
        rows.extend(cjk_row(i));
        i += 1;
    }
    // 열 정렬 마커: 좁은/넓은 혼합 + 줄 경계에 걸리는 wide 문자(선두 스페이서 경로).
    rows.extend_from_slice("CJK-MIX abc\u{1b}[35m가나다\u{1b}[0mdef END|\r\n".as_bytes());
    let edge: Vec<u8> = {
        let mut v = vec![b'a'; 79];
        v.extend_from_slice("가\r\n".as_bytes());
        v
    };
    rows.extend_from_slice(&edge);
    let deficit = RING + 1 - rows.len();
    rows.extend(pad_line(deficit));
    assert_eq!(rows.len(), RING + 1, "절단 정렬이 픽스처의 전제");

    // 이 픽스처의 판정(오른쪽 여백의 wide 문자가 다음 줄로 넘어간다)은 **자동 줄바꿈이 켜져 있다**는
    // 전제 위에 선다. 그 전제를 기본값에 맡기지 않고 스트림이 스스로 선언한다(DECSET 7) — 골든이
    // "기본값이 무엇이냐"는 별개의 질문에 걸리지 않게. 계약의 출생 상태에서도 이미 켜짐이므로
    // (SPEC.md §11.I) 이 시퀀스는 화면을 바꾸지 않는다: 픽스처를 자립시킬 뿐이다.
    let mut stream = b"\x1b[?7h".to_vec();
    stream.extend_from_slice(b"CJK-SEED\r\n");
    stream.extend_from_slice(&rows);
    stream
}

// ── ③ alt-screen 진입/이탈 ───────────────────────────────────────────────────

fn alt_screen() -> Vec<u8> {
    // 프라임 화면에 스크롤백 마커 → alt 진입 → 프레임 스팸(> RING, 1049h 가 링 밖으로
    // 밀려남) → 최종 TUI 프레임 + 커서 숨김.
    fn frame(k: usize) -> Vec<u8> {
        let mut v = b"\x1b[H".to_vec();
        for row in 1..=22usize {
            v.extend(format!("\x1b[{row};1H").into_bytes());
            v.extend(format!("F{k:03}-{row:02}:").into_bytes());
            for j in 0..36 {
                let r = 100 + ((k * 7 + row * 3 + j * 13) % 156);
                let g = 100 + ((k * 5 + row * 11 + j * 3) % 156);
                let b = 100 + ((k * 13 + row * 7 + j * 17) % 156);
                v.extend(format!("\x1b[38;2;{r};{g};{b}m나").into_bytes());
            }
            v.extend_from_slice(b"\x1b[0m");
        }
        v
    }

    let mut stream = Vec::new();
    // 화면(24행)보다 많은 줄 — 일부가 실제 스크롤백으로 밀려 들어간 상태에서 alt 진입.
    for n in 1..=30 {
        stream.extend(format!("SCROLLBACK-MARK line{n}\r\n").into_bytes());
    }
    stream.extend_from_slice(b"\x1b[?1049h");
    let alt_start = stream.len();
    let mut k = 0;
    while stream.len() - alt_start < RING + 500 {
        stream.extend(frame(k));
        k += 1;
    }
    stream.extend_from_slice(b"\x1b[2J\x1b[H");
    stream.extend_from_slice("\u{250c} TUI-FINAL-MARK \u{2510}\r\n".as_bytes());
    stream.extend_from_slice("\u{2502} \u{1b}[33minner\u{1b}[0m body \u{2502}\r\n".as_bytes());
    stream.extend_from_slice("\u{2514}\u{2500}\u{2500}\u{2500}\u{2518}".as_bytes());
    stream.extend_from_slice(b"\x1b[2;3H\x1b[?25l");
    stream
}

// ── ④ private mode — 링 창 밖에서도 산다 ─────────────────────────────────────

fn private_modes() -> Vec<u8> {
    let mut stream = Vec::new();
    // 세션 초기에 켜진 모드들 — 이후 출력이 링 용량을 넘어 세트 시퀀스가 창 밖으로 밀린다.
    stream.extend_from_slice(b"\x1b[?2004h\x1b[?1002h\x1b[?1006h\x1b[?1h\x1b=\x1b[?1004h\x1b[?1007h");
    stream.extend_from_slice(b"MODES-SET-MARK\r\n");
    for i in 0..1200 {
        stream.extend(heavy_row(i));
    }
    stream.extend_from_slice(b"AFTER-FILL-MARK\r\n");
    assert!(stream.len() > RING + 4096, "픽스처 전제: 모드 세트가 링 창 밖");
    stream
}

// ── ⑤ replay guard — 질의는 삼켜지고 재생에 실리지 않는다 ──────────────────────

fn replay_guard() -> Vec<u8> {
    // DA1/DA2/DSR/OSC 질의가 세션 출력에 들어 있었다. 라이브 때는 프론트 터미널이 한 번
    // 응답했다(단일 응답자). 복원 재생에서 두 번째 응답이 나오면 셸 stdin 오염이다.
    let mut stream = b"GUARD-MARK\r\n".to_vec();
    stream.extend_from_slice(b"\x1b[c"); // DA1
    stream.extend_from_slice(b"\x1b[>c"); // DA2
    stream.extend_from_slice(b"\x1b[6n"); // DSR 커서 질의
    stream.extend_from_slice(b"\x1b]11;?\x07"); // OSC 배경색 질의
    stream.extend_from_slice(b"AFTER-QUERY-MARK\r\n");
    stream
}

// ── ⑥ cold paint — 죽은 세션 체크포인트가 TUI 를 텍스트로 담는다 ───────────────

fn cold_paint_alt() -> Vec<u8> {
    let mut stream = Vec::new();
    // 프라임 화면(24행)보다 많은 스크롤백 마커 → 일부가 스크롤백으로 밀린 채 alt 진입.
    for n in 1..=30 {
        stream.extend(format!("SCROLL-{n}\r\n").into_bytes());
    }
    stream.extend_from_slice(b"\x1b[?1049h");
    stream.extend_from_slice(b"\x1b[2J\x1b[H");
    for i in 1..=10 {
        stream.extend(format!("\x1b[{i};1HTUI-LINE-{i}").into_bytes());
    }
    stream
}

// ── ⑦ DEC line drawing ───────────────────────────────────────────────────────

fn dec_line_drawing() -> Vec<u8> {
    // ESC(0 = G0 을 DEC special graphics 로 지정. 활성은 G0 이라 lqk/x/mqj 가 박스가 된다.
    // 내부 텍스트는 SO(→G1=ASCII)로 전환해 그린다(라인드로잉 아래 'n'→┼ 오염 방지).
    let mut stream = b"BORDER-BOX\r\n".to_vec();
    stream.extend_from_slice(b"\x1b(0"); // designate G0 = line drawing
    stream.extend_from_slice(b"lqqqqqqqqk\r\n"); // ┌────────┐
    stream.extend_from_slice(b"x"); // │ (G0 line drawing)
    stream.extend_from_slice(b"\x0e inner  \x0f"); // SO→G1(ASCII) " inner  " SI→G0
    stream.extend_from_slice(b"x\r\n"); // │
    stream.extend_from_slice(b"mqqqqqqqqj\r\n"); // └────────┘
    stream.extend_from_slice(b"\x1b(B"); // restore G0 = ASCII
    stream.extend_from_slice(b"DONE\r\n");
    stream
}
