//! soksak-contract-terminal — 계약 `soksak-spec-sidecar-terminal` 과 그 합격시험.
//!
//! **실행되는 것이 없다.** 배포물 0(dist·바이너리·레지스트리 등재 없음). 소비는 오직 빌드·테스트
//! 타임 dev-dependency 다. 이름의 `terminal` 은 이 계약이 규율하는 도메인이고, 계약 식별자 자체는
//! `soksak-spec-sidecar-terminal` 로 불변이다(문자열 값이지 배포 단위 이름이 아니다).
//!
//! **엔진이 없다.** 이 크레이트는 어떤 VT 엔진에도 의존하지 않는다. 정답은 엔진이 하는 짓이
//! 아니라 [`goldens`](../goldens) 에 **선언된 화면 상태**다 — 그래서 어느 구현체도 1급이 아니고,
//! 전 엔진이 동등한 후보로 같은 골든에 채점된다. 골든의 근거는 SPEC.md §11(정규형)·§12(골든)에
//! 적혀 있고, 그 판정이 곧 표준 제정이다.
//!
//! 채점 3축(엔진 불요):
//!   1. **해석 적합성** — 코퍼스 스트림을 먹은 미러의 [`ScreenState`] == 골든.
//!   2. **복원 적합성** — 그 미러의 `rehydrate` 페인트를 **신선한 같은 미러**에 먹인 뒤의
//!      [`ScreenState`] == **같은 골든**. 골든이 바깥에 있으므로 해석·복원이 나란히 틀리는
//!      자기-일관 오류도 숨지 못한다.
//!   3. **재생 가드** — 위 과정에서 PTY 로 나간 바이트 0, 재생 페인트에 질의 바이트 0.

pub mod bench;
pub mod corpus;
pub mod daemon_demand;
pub mod golden;
pub mod state;

pub use corpus::{Fixture, COLS, ROWS};
pub use state::{Attrs, Cell, Color, Modes, Row, ScreenState};

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
}

/// 재생 페인트에 실려서는 안 되는 질의 바이트(이중응답 원천 차단).
const QUERY_BYTES: [&[u8]; 4] = [b"\x1b[c", b"\x1b[>c", b"\x1b[6n", b"\x1b]11;?"];

/// 한 건의 합격시험. 골든에 대해 해석·복원·재생 가드를 모두 확인한다.
///
/// 평범한 단언 함수다 — 러너도 매크로 마법도 없다. 유닛은 `#[test]` 하나에서 이걸 부른다.
pub fn assert_conforms<M: MirrorUnderTest>(fixture: Fixture) {
    let stream = fixture.stream();

    // ── 1. 해석 적합성 — 스트림을 먹은 화면이 선언된 골든과 같은가.
    let mut mirror = M::new(COLS, ROWS);
    mirror.feed(&stream);
    let interpreted = mirror.screen_state();
    let expected = golden::load(fixture.stem());
    assert_states_eq(&expected, &interpreted, fixture, "해석");

    // ⑤ 는 질의를 삼켰다는 관찰이 픽스처의 본문이다.
    if fixture == Fixture::ReplayGuard {
        assert!(
            mirror.suppressed_replies() > 0,
            "{}: 미러가 질의를 보고 삼켰음이 관찰돼야 한다",
            fixture.stem()
        );
    }

    // ── 2. 복원 적합성 — 재생 페인트를 신선한 미러에 먹이면 같은 골든이 나오는가.
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
        // 원본과 복원본 양쪽에 같은 이탈 국면을 먹인다 — 둘 다 같은 골든이어야 한다. 복원본이
        // alt 밑에 얼려 운반한 프라임 화면이 실재해야만 통과한다.
        let after = golden::load(&format!("{}.after", fixture.stem()));
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
        let expected_cold = golden::load(&format!("{}.cold", fixture.stem()));
        assert_states_eq(&expected_cold, &sealed.screen_state(), fixture, "cold");
        assert_eq!(
            sealed.suppressed_replies(),
            0,
            "{}: cold 페인트에도 질의가 실리지 않는다",
            fixture.stem()
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
    assert_eq!(expected.modes, actual.modes, "{f}/{phase}: private mode 집합");
    assert_eq!(
        expected.history.len(),
        actual.history.len(),
        "{f}/{phase}: 스크롤백 행 수"
    );
    for (i, (e, a)) in expected.history.iter().zip(actual.history.iter()).enumerate() {
        assert_row_eq(e, a, &format!("{f}/{phase}: 스크롤백 H{i}"));
    }
    assert_eq!(
        expected.visible.len(),
        actual.visible.len(),
        "{f}/{phase}: 보이는 행 수"
    );
    for (i, (e, a)) in expected.visible.iter().zip(actual.visible.iter()).enumerate() {
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

/// 골든 부트스트랩·갱신 — 유닛이 자기 엔진으로 코퍼스를 돌려 정규형 텍스트를 내놓는다.
/// 산출물을 **그대로 신뢰해 굳히지 마라**: 엔진끼리 대조하고 VT 스펙(ctlseqs)과 견준 뒤에만
/// 골든이 된다(SPEC.md §12).
pub fn dump<M: MirrorUnderTest>(fixture: Fixture) -> Vec<(String, String)> {
    let stream = fixture.stream();
    let mut out = Vec::new();

    let mut mirror = M::new(COLS, ROWS);
    mirror.feed(&stream);
    out.push((fixture.stem().to_string(), golden::to_text(&mirror.screen_state())));

    // 복원본도 함께 낸다(`<이름>.restored`). 해석은 맞는데 복원이 어긋나는 결함은 이 둘을 나란히
    // 놓아야 보인다 — 골든 후보가 아니라 진단용이다(설치하지 않는다).
    let mut restored = M::new(COLS, ROWS);
    restored.feed(&mirror.rehydrate());
    out.push((
        format!("{}.restored", fixture.stem()),
        golden::to_text(&restored.screen_state()),
    ));

    if fixture == Fixture::ColdPaintAlt {
        let mut sealed = M::new(COLS, ROWS);
        sealed.feed(&mirror.cold_paint());
        out.push((
            format!("{}.cold", fixture.stem()),
            golden::to_text(&sealed.screen_state()),
        ));
    }

    if let Some(epilogue) = fixture.epilogue() {
        mirror.feed(&epilogue);
        out.push((
            format!("{}.after", fixture.stem()),
            golden::to_text(&mirror.screen_state()),
        ));
    }

    out
}
