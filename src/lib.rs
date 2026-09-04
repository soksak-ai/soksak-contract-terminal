//! soksak-contract-terminal — 계약 `soksak-spec-sidecar-terminal` 과 그 합격시험.
//!
//! **실행되는 것이 없다.** 배포물 0(dist·바이너리·레지스트리 등재 없음). 소비는 오직 빌드·테스트
//! 타임 dev-dependency 다. 이름의 `terminal` 은 이 계약이 규율하는 도메인이고, 계약 식별자 자체는
//! `soksak-spec-sidecar-terminal` 로 불변이다(문자열 값이지 배포 단위 이름이 아니다).
//!
//! **엔진이 없다.** 이 크레이트는 어떤 VT 엔진에도 의존하지 않는다. 정답은 엔진이 하는 짓이
//! 아니라 [`reference_states`](../reference_states) 에 **선언된 화면 상태**다 — 그래서 어느 구현체도 1급이 아니고,
//! 전 엔진이 동등한 후보로 같은 reference state에 채점된다. reference state의 근거는 SPEC.md §11(정규형)·§12(reference state)에
//! 적혀 있고, 그 판정이 곧 표준 제정이다.
//!
//! 채점 3축(엔진 불요):
//!   1. **해석 적합성** — 코퍼스 스트림을 먹은 미러의 [`ScreenState`] == reference state.
//!   2. **복원 적합성** — 그 미러의 `rehydrate` 페인트를 **신선한 같은 미러**에 먹인 뒤의
//!      [`ScreenState`] == **같은 reference state**. reference state이 바깥에 있으므로 해석·복원이 나란히 틀리는
//!      자기-일관 오류도 숨지 못한다.
//!   3. **재생 가드** — 위 과정에서 PTY 로 나간 바이트 0, 재생 페인트에 질의 바이트 0.

pub mod bench;
pub mod corpus;
pub mod frame;
pub mod reference_state;
pub mod state;

pub use corpus::{COLS, Fixture, ROWS};
pub use state::{Attrs, Cell, Color, CursorShape, CursorStyle, ModeReport, Modes, Row, ScreenState};

/// 피시험 미러의 면 — 합격시험이 유닛을 만지는 유일한 통로. 유닛의 엔진·내부 타입은 이 면 뒤에
/// 남는다(시험은 엔진을 모른다).
///
/// [`MirrorUnderTest::screen_state`] 가 이 계약의 요구 중 유일하게 새로 생긴 것이다: 유닛이 자기
/// 엔진의 표현을 계약의 **정규형**([`ScreenState`], SPEC.md §11)으로 변환해 내놓는다. 변환은 유닛
/// 좌석의 책임이다 — 계약은 엔진 표현을 알지 못한다.
pub trait MirrorUnderTest {
    /// 신선한 미러(격자 cols×rows).
    fn new(cols: u16, rows: u16) -> Self;

    /// 세션 출력 바이트 소비. 미러는 절대 응답하지 않는다.
    fn feed(&mut self, bytes: &[u8]);

    /// 격자 변경(resize 는 제어 op — tee 는 크기를 나르지 않는다).
    fn resize(&mut self, cols: u16, rows: u16);

    /// warm 재부착 재생 시퀀스.
    fn rehydrate(&self) -> Vec<u8>;

    /// cold 체크포인트 페인트 — 화면 이력을 비활성 텍스트로 평면화한 시퀀스.
    fn cold_paint(&self) -> Vec<u8>;

    /// 미러가 삼킨 응답 요구 수. 관찰 전용 — 나가는 바이트는 0 이다.
    fn suppressed_replies(&self) -> u64;

    /// 현재 화면 상태를 계약의 정규형으로.
    fn screen_state(&self) -> ScreenState;

    /// 엔진이 해석한 현재 cursor shape/blink 상태. Adapter가 CSI를 다시 파싱해 만든 값은
    /// 적합성 증거가 아니다.
    fn cursor_style(&self) -> CursorStyle;

    /// 재생만으로는 되살릴 수 없는 mode 상태. 소유자가 이것을 기록해 두었다가 복원 시 재생보다
    /// 먼저 적용한다.
    ///
    /// `screen_state()` 도 같은 사실을 담지만 그것은 격자 전체를 만든다. mode 는 프로그램이
    /// 전체 화면 모드에 들고 날 때 바뀌므로 그 시점마다 격자를 만드는 것은 값이 맞지 않는다.
    fn mode_report(&self) -> ModeReport {
        let state = self.screen_state();
        ModeReport::of(state.modes, state.alt)
    }
}

/// DEC VT520 DECSCUSR and xterm's bar extension (`CSI Ps SP q`). Ps 0 is an
/// implementation-configured default and is deliberately absent: a contract
/// cannot turn one engine's default preference into a terminal standard.
pub const DECSCUSR_CASES: [(u8, CursorStyle); 6] = [
    (1, CursorStyle { shape: CursorShape::Block, blinking: true }),
    (2, CursorStyle { shape: CursorShape::Block, blinking: false }),
    (3, CursorStyle { shape: CursorShape::Underline, blinking: true }),
    (4, CursorStyle { shape: CursorShape::Underline, blinking: false }),
    (5, CursorStyle { shape: CursorShape::Bar, blinking: true }),
    (6, CursorStyle { shape: CursorShape::Bar, blinking: false }),
];

/// Shared cursor acceptance case. It grades the engine's public state and the
/// warm rehydrate bytes; it never parses the session stream on an adapter's
/// behalf.
pub fn assert_cursor_style_conforms<M: MirrorUnderTest>() {
    for (parameter, expected) in DECSCUSR_CASES {
        let sequence = format!("\x1b[{parameter} q");
        let mut mirror = M::new(COLS, ROWS);
        mirror.feed(sequence.as_bytes());
        assert_eq!(
            mirror.cursor_style(),
            expected,
            "DECSCUSR Ps={parameter}: engine cursor state"
        );

        let paint = mirror.rehydrate();
        let mut restored = M::new(COLS, ROWS);
        restored.feed(&paint);
        assert_eq!(
            restored.cursor_style(),
            expected,
            "DECSCUSR Ps={parameter}: warm rehydrate cursor state"
        );
    }

    // DECTCEM changes visibility only. The selected shape and blink state
    // survive both hide and show.
    let selected = CursorStyle { shape: CursorShape::Bar, blinking: false };
    let mut visibility = M::new(COLS, ROWS);
    visibility.feed(b"\x1b[6 q");
    visibility.feed(b"\x1b[?25l");
    assert!(!visibility.screen_state().modes.show_cursor, "DECTCEM hide");
    assert_eq!(visibility.cursor_style(), selected, "DECTCEM hide preserves style");
    visibility.feed(b"\x1b[?25h");
    assert!(visibility.screen_state().modes.show_cursor, "DECTCEM show");
    assert_eq!(visibility.cursor_style(), selected, "DECTCEM show preserves style");

    // Xterm DEC private mode 12 changes blink without changing the selected
    // shape. It is separate from DECTCEM.
    let mut blink = M::new(COLS, ROWS);
    blink.feed(b"\x1b[6 q");
    blink.feed(b"\x1b[?12h");
    assert_eq!(
        blink.cursor_style(),
        CursorStyle { shape: CursorShape::Bar, blinking: true },
        "DECSET 12 starts cursor blinking"
    );
    blink.feed(b"\x1b[?12l");
    assert_eq!(blink.cursor_style(), selected, "DECRST 12 stops cursor blinking");
}

/// 기록된 mode 보고를 새 미러에 적용하면 그 미러가 같은 보고를 답한다.
///
/// 이것이 S4-5 의 두 번째 절반입니다. 링 창 밖에서 켜진 mode 는 저장된 어느 바이트에도 없으므로,
/// 재생만으로 만든 미러는 세션이 남긴 mode 가 아니라 자기 기본값에 있습니다. 보고를 재생보다 먼저
/// 적용해야 그 뒤의 바이트가 맞는 화면과 맞는 mode 에 그려집니다.
pub fn assert_mode_report_restores<M: MirrorUnderTest>() {
    // 링 용량과 무관하게, mode 를 켠 바이트가 재생에 없는 경우를 그대로 만든다.
    let mut live = M::new(COLS, ROWS);
    live.feed(b"\x1b[?2004h\x1b[?1002h\x1b[?1006h\x1b[?1h\x1b=\x1b[?1004h\x1b[?1007h\x1b[?25l");
    let recorded = live.mode_report();

    let mut restored = M::new(COLS, ROWS);
    restored.feed(&recorded.apply_bytes());
    assert_eq!(
        restored.mode_report(),
        recorded,
        "a mirror the report was applied to reports the same modes"
    );

    // 보고를 적용하지 않은 미러는 같은 답을 내지 않는다. 이것이 없으면 위의 단언은 두 기본값이
    // 같다는 말일 뿐이다.
    let untouched = M::new(COLS, ROWS);
    assert_ne!(
        untouched.mode_report(),
        recorded,
        "the fixture must set modes a fresh mirror does not already have"
    );

    // alt-screen 은 보고의 일부다. 한 화면의 mode 를 다른 화면에 복원하면 그 뒤의 재생이 틀린
    // 화면에 그려진다.
    let mut alt = M::new(COLS, ROWS);
    alt.feed(b"\x1b[?1049h\x1b[?2004h");
    let alt_report = alt.mode_report();
    assert!(alt_report.alt, "the fixture must be on the alternate screen");
    let mut alt_restored = M::new(COLS, ROWS);
    alt_restored.feed(&alt_report.apply_bytes());
    assert_eq!(
        alt_restored.mode_report(),
        alt_report,
        "the alternate screen and its modes are restored together"
    );
}

/// 재생 페인트에 실려서는 안 되는 질의 바이트(이중응답 원천 차단).
const QUERY_BYTES: [&[u8]; 4] = [b"\x1b[c", b"\x1b[>c", b"\x1b[6n", b"\x1b]11;?"];

/// 한 건의 합격시험. reference state에 대해 해석·복원·재생 가드를 모두 확인한다.
///
/// 평범한 단언 함수다 — 러너도 매크로 마법도 없다. 유닛은 `#[test]` 하나에서 이걸 부른다.
pub fn assert_conforms<M: MirrorUnderTest>(fixture: Fixture) {
    let stream = fixture.stream();

    // ── 1. 해석 적합성 — 스트림을 먹은 화면이 선언된 reference state과 같은가.
    let mut mirror = M::new(COLS, ROWS);
    mirror.feed(&stream);
    let interpreted = mirror.screen_state();
    let expected = reference_state::load(fixture.stem());
    assert_states_eq(&expected, &interpreted, fixture, "해석");

    // ⑤ 는 질의를 삼켰다는 관찰이 픽스처의 본문이다.
    if fixture == Fixture::ReplayGuard {
        assert!(
            mirror.suppressed_replies() > 0,
            "{}: 미러가 질의를 보고 삼켰음이 관찰돼야 한다",
            fixture.stem()
        );
    }

    // ── 2. 복원 적합성 — 재생 페인트를 신선한 미러에 먹이면 같은 reference state이 나오는가.
    let paint = mirror.rehydrate();
    assert_no_queries(&paint, fixture, "rehydrate");
    let mut restored = M::new(COLS, ROWS);
    restored.feed(&paint);
    assert_states_eq(&expected, &restored.screen_state(), fixture, "복원");

    // ── 3. 재생 가드 — 재생 페인트는 질의를 담지 않으므로 삼킬 것도 없다.
    assert_eq!(
        restored.suppressed_replies(),
        0,
        "{}: 재생 페인트에 질의가 없어야 하므로 삼킨 응답 요구도 0 이어야 한다",
        fixture.stem()
    );

    // ── 4. 국면이 더 있는 픽스처.
    if let Some(epilogue) = fixture.epilogue() {
        // 원본과 복원본 양쪽에 같은 이탈 국면을 먹인다 — 둘 다 같은 reference state이어야 한다. 복원본이
        // alt 밑에 얼려 운반한 프라임 화면이 실재해야만 통과한다.
        let after = reference_state::load(&format!("{}.after", fixture.stem()));
        mirror.feed(&epilogue);
        assert_states_eq(&after, &mirror.screen_state(), fixture, "이탈(원본)");
        restored.feed(&epilogue);
        assert_states_eq(&after, &restored.screen_state(), fixture, "이탈(복원본)");
    }

    // ── 5. cold 체크포인트 — 평면화한 페인트가 선언된 화면을 되살리는가.
    if fixture == Fixture::ColdPaintAlt {
        let cold = mirror_cold::<M>(&stream);
        assert_no_queries(&cold, fixture, "cold_paint");
        let mut sealed = M::new(COLS, ROWS);
        sealed.feed(&cold);
        let expected_cold = reference_state::load(&format!("{}.cold", fixture.stem()));
        assert_states_eq(&expected_cold, &sealed.screen_state(), fixture, "cold");
        assert_eq!(
            sealed.suppressed_replies(),
            0,
            "{}: cold 페인트에도 질의가 실리지 않는다",
            fixture.stem()
        );
    }
}

/// resize→rehydrate **폭 정합** — 모든 엔진이 통과해야 하는 공유 단언(개별 엔진에 복붙하지 않는다).
///
/// 배경: warm 재부착 화면은 미러 그리드를 SGR 런으로 합성한 것이고, tee 는 크기를 안 나르며 코어
/// resize 는 데몬 PTY 만 바꿔 미러엔 전파하지 않는다. 그래서 소비자(kit)는 rehydrate 직전(그리고
/// 리사이즈마다) 계약 `resize` op 로 미러를 pane 폭에 맞춘다. 이 단언은 그 전제 — "미러를 다른 폭으로
/// resize 한 뒤 rehydrate 하면, 그 재생 페인트를 신선한 **같은 폭** 미러에 먹인 화면이 resize 된 원본과
/// 정규형으로 같고(왕복 충실), 논리 내용이 보존된다" — 를 엔진 불가지로 못박는다. 깨지면 좁아진 pane 의
/// warm 복원이 격자를 깬다(실측). reference state 불요: 재생 화면을 resize 된 원본과 자기대조하고, 내용 보존은
/// 입력 줄이 재감김 결과에 온전히 남는지로 확인한다(reflow 가 손실·뒤섞음이 아님).
pub fn assert_resize_reflow<M: MirrorUnderTest>() {
    // COLS(80) 한 행을 넘는 논리 줄(120자) — 재감김이 일어나야 검사가 의미 있다. 공백 없는 패턴이라
    // 꼬리 공백 제거가 내용을 지우지 않는다.
    let width = COLS as usize + COLS as usize / 2;
    let text: Vec<u8> = (0..width).map(|i| b'a' + (i % 26) as u8).collect();
    let original = String::from_utf8(text.clone()).unwrap();
    let mut stream = text;
    stream.extend_from_slice(b"\r\n");

    // 축소·항등·확대 세 방향 모두 폭에 정확해야 한다.
    for &target in &[COLS / 2, COLS, COLS * 2] {
        let cols = target.max(2);
        let mut m = M::new(COLS, ROWS);
        m.feed(&stream);
        m.resize(cols, ROWS);

        // ① 왕복 충실 — 재생 페인트를 신선한 같은 폭 미러에 먹이면 resize 된 원본과 정규형이 같다.
        let paint = m.rehydrate();
        let mut fresh = M::new(cols, ROWS);
        fresh.feed(&paint);
        let (want, got) = (m.screen_state(), fresh.screen_state());
        assert_eq!(
            want, got,
            "resize→{cols}폭 후 rehydrate 재도색이 resize 된 원본과 어긋남(왕복 충실 실패)"
        );
        assert_eq!(
            got.cols, cols,
            "resize→{cols}폭: 재생 화면 폭이 목표와 다름"
        );

        // ② 내용 보존 — resize 는 재감김일 뿐 손실이 아니다. 스크롤백+화면 행 텍스트를 이으면(꼬리
        // 공백 제거) 원본 줄이 그 안에 온전히 남는다.
        let joined: String = got
            .history
            .iter()
            .chain(got.visible.iter())
            .map(|r| r.text().trim_end().to_string())
            .collect::<Vec<_>>()
            .join("");
        assert!(
            joined.contains(&original),
            "resize→{cols}폭 후 내용 보존 실패: 원본이 재감김 결과 {joined:?} 에 온전히 없음"
        );
    }
}

fn mirror_cold<M: MirrorUnderTest>(stream: &[u8]) -> Vec<u8> {
    let mut m = M::new(COLS, ROWS);
    m.feed(stream);
    m.cold_paint()
}

fn assert_no_queries(paint: &[u8], fixture: Fixture, what: &str) {
    for q in QUERY_BYTES {
        assert!(
            !paint.windows(q.len()).any(|w| w == q),
            "{}: {what} 페인트에 질의 {q:?} 가 실렸다(이중응답)",
            fixture.stem()
        );
    }
}

// 정규형 비교 — 어긋난 첫 지점을 사람이 읽을 수 있게 짚어 준다(수천 셀의 통짜 덤프 금지).
fn assert_states_eq(expected: &ScreenState, actual: &ScreenState, fixture: Fixture, phase: &str) {
    let f = fixture.stem();
    assert_eq!(expected.cols, actual.cols, "{f}/{phase}: cols");
    assert_eq!(expected.rows, actual.rows, "{f}/{phase}: rows");
    assert_eq!(expected.alt, actual.alt, "{f}/{phase}: alt-screen 활성");
    assert_eq!(expected.cursor, actual.cursor, "{f}/{phase}: 커서(x, y)");
    assert_eq!(
        expected.modes, actual.modes,
        "{f}/{phase}: private mode 집합"
    );
    assert_eq!(
        expected.history.len(),
        actual.history.len(),
        "{f}/{phase}: 스크롤백 행 수"
    );
    for (i, (e, a)) in expected
        .history
        .iter()
        .zip(actual.history.iter())
        .enumerate()
    {
        assert_row_eq(e, a, &format!("{f}/{phase}: 스크롤백 H{i}"));
    }
    assert_eq!(
        expected.visible.len(),
        actual.visible.len(),
        "{f}/{phase}: 보이는 행 수"
    );
    for (i, (e, a)) in expected
        .visible
        .iter()
        .zip(actual.visible.iter())
        .enumerate()
    {
        assert_row_eq(e, a, &format!("{f}/{phase}: 보이는 화면 V{i}"));
    }
}

fn assert_row_eq(expected: &Row, actual: &Row, ctx: &str) {
    if expected == actual {
        return;
    }
    // 텍스트가 먼저 갈리면 그걸 보여 준다(스타일 diff 보다 읽힌다).
    let (et, at) = (expected.text(), actual.text());
    assert_eq!(et, at, "{ctx}: 텍스트");
    // 텍스트는 같은데 셀이 다르다 — 첫 어긋난 칸을 짚는다.
    for (i, (e, a)) in expected.0.iter().zip(actual.0.iter()).enumerate() {
        assert_eq!(e, a, "{ctx}: {i}번 칸(스타일)");
    }
    assert_eq!(expected.0.len(), actual.0.len(), "{ctx}: 칸 수");
}

/// reference state 부트스트랩·갱신 — 유닛이 자기 엔진으로 코퍼스를 돌려 정규형 텍스트를 내놓는다.
/// 산출물을 **그대로 신뢰해 굳히지 마라**: 엔진끼리 대조하고 VT 스펙(ctlseqs)과 견준 뒤에만
/// reference state이 된다(SPEC.md §12).
pub fn dump<M: MirrorUnderTest>(fixture: Fixture) -> Vec<(String, String)> {
    let stream = fixture.stream();
    let mut out = Vec::new();

    let mut mirror = M::new(COLS, ROWS);
    mirror.feed(&stream);
    out.push((
        fixture.stem().to_string(),
        reference_state::to_text(&mirror.screen_state()),
    ));

    // 복원본도 함께 낸다(`<이름>.restored`). 해석은 맞는데 복원이 어긋나는 결함은 이 둘을 나란히
    // 놓아야 보인다 — reference state 후보가 아니라 진단용이다(설치하지 않는다).
    let mut restored = M::new(COLS, ROWS);
    restored.feed(&mirror.rehydrate());
    out.push((
        format!("{}.restored", fixture.stem()),
        reference_state::to_text(&restored.screen_state()),
    ));

    if fixture == Fixture::ColdPaintAlt {
        let mut sealed = M::new(COLS, ROWS);
        sealed.feed(&mirror.cold_paint());
        out.push((
            format!("{}.cold", fixture.stem()),
            reference_state::to_text(&sealed.screen_state()),
        ));
    }

    if let Some(epilogue) = fixture.epilogue() {
        mirror.feed(&epilogue);
        out.push((
            format!("{}.after", fixture.stem()),
            reference_state::to_text(&mirror.screen_state()),
        ));
    }

    out
}
