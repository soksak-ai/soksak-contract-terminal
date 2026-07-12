//! 벤치 — 같은 코퍼스, 같은 면, 전 엔진 유닛 공통. 계약이 소유한다(유닛에 벤치 사본 없음).
//!
//! 프레임워크를 쓰지 않는다: 반복하고, 중앙값을 잡고, 표로 찍는다. 그 이상은 이 계약이 답해야
//! 하는 질문("어느 엔진이 이 일을 얼마에 하는가")에 보태는 것이 없다.
//!
//! ## 축 넷
//!   ① **feed 처리량**(MB/s) — tee 소비 경로. 미러가 세션 출력을 얼마나 빨리 먹는가.
//!   ② **rehydrate**(지연 + 페인트 바이트) — warm 재부착의 비용. 크기는 소켓으로 나가는 양이다.
//!   ③ **cold_paint**(지연 + 봉인 바이트) — 체크포인트의 비용. 크기는 디스크에 앉는 양이다.
//!   ④ **메모리**(바이트) — 스크롤백 창을 가득 채운 미러 하나가 실제로 붙들고 있는 힙.
//!
//! ## ④ 를 어떻게 재는가 — 두 숫자를 함께 낸다
//! **heap**: [`CountingAlloc`] 을 global allocator 로 끼워, 미러를 살려 두는 동안 늘어난 순 할당
//! 바이트를 센다. 정확하지만 **Rust 할당자를 지나는 것만** 본다.
//!
//! **rss**: 프로세스의 상주 메모리 증가분(`ps -o rss=`). 덜 정밀하고(페이지 단위·반환 정책에
//! 좌우된다) 프로세스 전체를 보지만, **할당자를 우회하는 메모리까지** 본다.
//!
//! 둘 다 내는 이유는 실측이다: 어떤 엔진은 그리드 페이지를 **mmap 으로 직접** 잡는다 — 그 메모리는
//! Rust 수준 계수기에 원리적으로 잡히지 않아 heap 이 0 에 가깝게 나온다. 그 엔진의 실제 사용량은
//! rss 만 말해 준다. 한 숫자만 봤다면 "메모리를 안 쓰는 엔진"이라는 거짓 결론에 닿았을 것이다.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use crate::corpus::{Fixture, COLS, ROWS};
use crate::MirrorUnderTest;

// ── ④ 측정 도구 — 순 할당 바이트를 세는 global allocator ──────────────────────

static ALLOCATED: AtomicUsize = AtomicUsize::new(0);

/// 벤치 바이너리가 끼우는 global allocator. 살아 있는 할당 바이트를 센다.
///
/// ```ignore
/// #[global_allocator]
/// static ALLOC: soksak_contract_terminal::bench::CountingAlloc =
///     soksak_contract_terminal::bench::CountingAlloc::new();
/// ```
pub struct CountingAlloc;

impl CountingAlloc {
    pub const fn new() -> Self {
        CountingAlloc
    }
}

impl Default for CountingAlloc {
    fn default() -> Self {
        Self::new()
    }
}

unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = System.alloc(layout);
        if !p.is_null() {
            ALLOCATED.fetch_add(layout.size(), Ordering::Relaxed);
        }
        p
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout);
        ALLOCATED.fetch_sub(layout.size(), Ordering::Relaxed);
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let p = System.realloc(ptr, layout, new_size);
        if !p.is_null() {
            ALLOCATED.fetch_add(new_size, Ordering::Relaxed);
            ALLOCATED.fetch_sub(layout.size(), Ordering::Relaxed);
        }
        p
    }
}

fn live_bytes() -> usize {
    ALLOCATED.load(Ordering::Relaxed)
}

/// 프로세스 상주 메모리(바이트). `ps` 는 KB 로 답한다. 재는 데 실패하면 0 — 무음으로 틀린 값을
/// 지어내지 않는다.
fn rss_bytes() -> usize {
    let pid = std::process::id().to_string();
    let out = std::process::Command::new("ps").args(["-o", "rss=", "-p", &pid]).output();
    match out {
        Ok(o) => String::from_utf8_lossy(&o.stdout)
            .trim()
            .parse::<usize>()
            .map(|kb| kb * 1024)
            .unwrap_or(0),
        Err(_) => 0,
    }
}

// ── 코퍼스 — 고정·재현 가능. 숫자가 비교 가능하려면 모두가 같은 것을 먹어야 한다 ──

/// 벤치 코퍼스. 합격시험의 일곱 스트림(링 절단·CJK·alt-screen·모드·질의·cold·charset)을 순서대로
/// 이어 붙이고, 거기에 **TUI 재그림**(전체 화면을 매 프레임 다시 칠하는 트루컬러 프레임 200장)을
/// 얹는다 — 실사용에서 미러를 가장 세게 때리는 것이 그 패턴이다(vim/tmux/htop).
///
/// 구성은 [`corpus_shape`] 가 숫자로 보고한다.
pub fn corpus() -> Vec<u8> {
    let mut out = Vec::new();
    for f in Fixture::ALL {
        out.extend(f.stream());
    }
    out.extend(tui_redraw(200));
    out
}

/// 화면 전체를 매번 다시 칠하는 TUI 프레임. 셀마다 색이 달라 스타일 테이블을 계속 흔든다.
fn tui_redraw(frames: usize) -> Vec<u8> {
    let mut out = Vec::new();
    for k in 0..frames {
        out.extend_from_slice(b"\x1b[H");
        for row in 1..=(ROWS as usize) {
            out.extend(format!("\x1b[{row};1H").into_bytes());
            for j in 0..(COLS as usize / 2) {
                let r = 40 + ((k * 3 + row * 7 + j * 11) % 200);
                let g = 40 + ((k * 5 + row * 11 + j * 3) % 200);
                let b = 40 + ((k * 7 + row * 13 + j * 5) % 200);
                out.extend(format!("\x1b[38;2;{r};{g};{b}m가").into_bytes());
            }
            out.extend_from_slice(b"\x1b[0m");
        }
    }
    out
}

/// 스크롤백 창을 가득 채우는 스트림(④ 메모리 축의 입력). 미러의 창(1000행)보다 넉넉히 많은
/// 행을, 스타일이 촘촘한 내용으로 민다 — 가벼운 텍스트로 재면 엔진 간 차이가 안 드러난다.
fn scrollback_fill() -> Vec<u8> {
    let mut out = Vec::new();
    for i in 0..1200 {
        out.extend(crate::corpus::heavy_row(i));
    }
    out
}

/// 코퍼스 구성 보고(문서가 아니라 숫자로).
pub fn corpus_shape() -> String {
    let c = corpus();
    let s = scrollback_fill();
    format!(
        "feed corpus {:.2} MB (fixtures {} + TUI redraw 200 frames) · scrollback fill {:.2} MB (1200 heavy rows)",
        c.len() as f64 / 1e6,
        Fixture::ALL.len(),
        s.len() as f64 / 1e6,
    )
}

// ── 측정 ─────────────────────────────────────────────────────────────────────

/// 한 유닛의 측정 결과.
#[derive(Debug, Clone)]
pub struct Report {
    pub unit: String,
    /// ① feed 처리량(MB/s, 중앙값).
    pub feed_mb_s: f64,
    /// ② rehydrate 지연(ms, 중앙값)과 페인트 크기(바이트).
    pub rehydrate_ms: f64,
    pub paint_bytes: usize,
    /// ③ cold_paint 지연(ms, 중앙값)과 봉인 크기(바이트).
    pub cold_ms: f64,
    pub cold_bytes: usize,
    /// ④ 스크롤백 창을 채운 미러 하나가 붙든 Rust 힙(바이트). 할당자를 우회하는 메모리는 못 본다.
    pub live_bytes: usize,
    /// ④ 같은 조건에서의 프로세스 상주 메모리 증가분(바이트). 우회 메모리까지 본다.
    pub rss_bytes: usize,
    /// **수요**(MB/s) — 이 기계에서 §6.2 의 tee 관이 미러 앞까지 배달할 수 있는 최대. 엔진과
    /// 무관하게 같은 실행에서 직접 잰다([`crate::demand`]). feed 예산이 여기서 나온다.
    pub demand_mb_s: f64,
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

/// 반복 횟수. 중앙값이 안정되기에 충분하고, 한 유닛이 몇 초 안에 끝난다.
const REPEATS: usize = 5;

/// 네 축을 잰다. 같은 머신·같은 빌드(release)·다른 부하 없는 상태를 전제한다.
pub fn run<M: MirrorUnderTest>(unit: &str) -> Report {
    // ④ 메모리 — **가장 먼저** 잰다. 뒤에 재면 앞 단계가 데워 놓은 힙을 할당자가 재사용해 RSS 가
    // 늘지 않고, 그러면 "메모리를 안 쓰는 엔진"이라는 거짓 숫자가 나온다(실측으로 확인했다).
    let fill = scrollback_fill();
    let before_heap = live_bytes();
    let before_rss = rss_bytes();
    let mut held = M::new(COLS, ROWS);
    held.feed(&fill);
    let live = live_bytes().saturating_sub(before_heap);
    let rss = rss_bytes().saturating_sub(before_rss);
    std::hint::black_box(&held);
    drop(held);
    drop(fill);

    let corpus = corpus();
    let corpus_mb = corpus.len() as f64 / 1e6;

    // ① feed — 신선한 미러가 코퍼스를 다 먹는 데 걸린 시간.
    let feed: Vec<f64> = (0..REPEATS)
        .map(|_| {
            let mut m = M::new(COLS, ROWS);
            let t = Instant::now();
            m.feed(&corpus);
            let secs = t.elapsed().as_secs_f64();
            corpus_mb / secs
        })
        .collect();

    // ②③ — 코퍼스를 먹은 미러에서 재생·봉인 페인트를 뽑는다.
    let mut m = M::new(COLS, ROWS);
    m.feed(&corpus);

    let rehydrate: Vec<f64> = (0..REPEATS)
        .map(|_| {
            let t = Instant::now();
            let p = m.rehydrate();
            let ms = t.elapsed().as_secs_f64() * 1e3;
            std::hint::black_box(&p);
            ms
        })
        .collect();
    let paint_bytes = m.rehydrate().len();

    let cold: Vec<f64> = (0..REPEATS)
        .map(|_| {
            let t = Instant::now();
            let p = m.cold_paint();
            let ms = t.elapsed().as_secs_f64() * 1e3;
            std::hint::black_box(&p);
            ms
        })
        .collect();
    let cold_bytes = m.cold_paint().len();
    drop(m);

    // 수요 — **가장 나중에** 잰다. 이 측정은 64 MB 를 만지므로 앞서 재면 ④ 의 RSS 증가분을
    // 통째로 오염시킨다(할당자가 데워지면 모두가 0 으로 나온다 — bench 모듈 머리말의 그 함정).
    //
    // 실 데몬으로 잰다. 데몬 바이너리가 없으면 **수요를 모르는 것이고, 수요를 모르면 판정할 수
    // 없다** — 조용히 넘어가지 않고 큰 소리로 죽는다.
    let bin = crate::daemon_demand::ptyd_bin().expect(
        "SOKSAK_PTYD_BIN 이 없다. 수요는 실 데몬이 tee 로 배달하는 속도이고, 그것을 모르면 feed \
         예산을 판정할 수 없다(SPEC.md §14.1). 유닛 게이트가 코어에서 데몬을 빌드해 주입한다.",
    );
    let demand_mb_s = crate::daemon_demand::detached_arrival_mb_s(&bin);

    Report {
        unit: unit.to_string(),
        feed_mb_s: median(feed),
        rehydrate_ms: median(rehydrate),
        paint_bytes,
        cold_ms: median(cold),
        cold_bytes,
        live_bytes: live,
        rss_bytes: rss,
        demand_mb_s,
    }
}

// ── 보고 — 한 줄 직렬화(유닛이 쓰고, 표가 읽는다) + 표 ─────────────────────────

impl Report {
    pub fn to_line(&self) -> String {
        format!(
            "{} {} {} {} {} {} {} {} {}",
            self.unit,
            self.feed_mb_s,
            self.rehydrate_ms,
            self.paint_bytes,
            self.cold_ms,
            self.cold_bytes,
            self.live_bytes,
            self.rss_bytes,
            self.demand_mb_s
        )
    }

    pub fn from_line(s: &str) -> Result<Report, String> {
        let f: Vec<&str> = s.split_whitespace().collect();
        if f.len() != 9 {
            return Err(format!("bench line has {} fields, want 9", f.len()));
        }
        let num = |i: usize| f[i].parse::<f64>().map_err(|e| e.to_string());
        let cnt = |i: usize| f[i].parse::<usize>().map_err(|e| e.to_string());
        Ok(Report {
            unit: f[0].to_string(),
            feed_mb_s: num(1)?,
            rehydrate_ms: num(2)?,
            paint_bytes: cnt(3)?,
            cold_ms: num(4)?,
            cold_bytes: cnt(5)?,
            live_bytes: cnt(6)?,
            rss_bytes: cnt(7)?,
            demand_mb_s: num(8)?,
        })
    }
}

/// 사람이 읽는 비교표. 축마다 최고값을 기준으로 상대 배수를 함께 찍는다 — 절대값만으로는
/// "이 엔진이 저 엔진보다 얼마나 무거운가"가 눈에 안 들어온다.
pub fn table(reports: &[Report]) -> String {
    let mut out = String::new();
    out.push_str(&format!("corpus: {}\n", corpus_shape()));
    out.push_str(&format!("repeats: {REPEATS} (median), release build\n\n"));
    // 수요는 유닛마다 따로 잰다(같은 기계·같은 관이므로 값은 서로 가깝다). 표는 중앙값을 쓴다 —
    // 이 표에 등수는 없다. 순위표로 읽으면 잘못 읽는 것이다(SPEC.md §14).
    let floor = demand_floor(reports);
    out.push_str(&format!(
        "demand (real daemon, detached tee arrival, this machine): {floor:.1} MB/s = the feed floor\n\n"
    ));
    out.push_str(&format!(
        "{:<12} {:>11} {:>7} {:>10} {:>10} {:>9} {:>9} {:>11} {:>9}\n",
        "unit", "feed MB/s", "vs floor", "rehyd ms", "paint KB", "cold ms", "cold KB", "heap MB", "rss MB"
    ));
    out.push_str(&"-".repeat(96));
    out.push('\n');

    for r in reports {
        out.push_str(&format!(
            "{:<12} {:>11.1} {:>7} {:>10.2} {:>10.1} {:>9.2} {:>9.1} {:>11.1} {:>9.1}\n",
            r.unit,
            r.feed_mb_s,
            if r.feed_mb_s >= floor { "ok" } else { "UNDER" },
            r.rehydrate_ms,
            r.paint_bytes as f64 / 1024.0,
            r.cold_ms,
            r.cold_bytes as f64 / 1024.0,
            r.live_bytes as f64 / 1e6,
            r.rss_bytes as f64 / 1e6,
        ));
    }
    out
}

/// 이 실행의 feed 하한 — 유닛들이 각자 잰 수요의 중앙값 × 비율. 유닛의 **성적은 전혀 보지 않는다**
/// (그것이 후보가 기준을 정하는 길이다). 보는 것은 관의 속도뿐이다.
pub fn demand_floor(reports: &[Report]) -> f64 {
    let mut d: Vec<f64> = reports.iter().map(|r| r.demand_mb_s).collect();
    d.sort_by(|a, b| a.partial_cmp(b).unwrap());
    d[d.len() / 2] * BUDGET_FEED_OF_DEMAND
}

// ── 예산 — 합격 게이트(SPEC.md §14.2) ────────────────────────────────────────
// 예산의 출처는 **요구**다(SPEC.md §14). 후보의 실측 분포에서 역산하지 않는다 — 그렇게 하면
// 기준을 후보가 정하게 되고, 전 후보가 함께 느려질 때 아무도 못 잡는다. 어겼다면 약화시키지
// 말고 원인을 찾아라(퇴행) 아니면 재보정하라(기계) — 재보정은 값을 낮추는 것이 아니라 요구를
// 다시 재는 것이다(scripts/frontend-demand.sh).

/// feed 예산 = **수요 그 자체**. 판정은 `feed >= demand` 다. 계수는 없다.
///
/// 유도(SPEC.md §14): 요구는 "미러가 tee gap 의 원인이 되지 않는다"이고, 수요는 **데몬이 tee 로
/// 실제 배달하는 지속 속도**다. 미러가 그보다 느리면 데몬은 반드시 떨군다 — 이 등식에 여유분을
/// 끼워 넣을 자리가 없다. 곱할 계수를 고르는 순간 그 계수는 후보를 보고 고른 것이 된다.
///
/// 수요는 [`crate::daemon_demand`] 가 **실 데몬**으로 그 기계에서 직접 잰다(모델이 아니다 —
/// 관을 흉내 낸 모델은 실제보다 2.4배 빠른 답을 냈다). 앱이 붙어 있지 않은 **분리 모드**로
/// 잰다: 그 모드에는 데몬의 읽기를 늦추는 것이 아무것도 없고(부착이 없으면 플로우 제어가 꺼진다),
/// 미러가 존재하는 이유가 바로 그 모드이기 때문이다. 두 모드 중 더 엄한 쪽이 요구다.
pub const BUDGET_FEED_OF_DEMAND: f64 = 1.0;
/// rehydrate·cold 지연(ms). 이 축은 엔진이 아니라 직렬화기를 잰다.
pub const BUDGET_PAINT_MS: f64 = 5.0;
/// 페인트·봉인 크기(바이트). 유도는 격자의 기하다(SPEC.md §14.2): 복원 창 80×1000 = 80,000 칸,
/// 칸마다 트루컬러 fg 전환(`ESC[38;2;R;G;Bm`, 최대 19바이트) + 3바이트 문자 = 22바이트 → 1.76 MB.
/// 그것이 이 격자가 담을 수 있는 **가장 무거운 화면**이고, 2 MiB 는 그 위에 놓인 천장이다.
pub const BUDGET_PAINT_BYTES: usize = 2 * 1024 * 1024;
/// 스크롤백 창을 채운 미러 하나의 상주 메모리 증가분. heap 은 게이트가 아니다(0 이 정상일 수 있다).
/// 유도: 사이드카는 한 워크스페이스의 모든 팬을 미러링하는 배경 서비스다. 팬 16개를 512 MB 안에
/// 담겠다는 약속이 미러 하나당 32 MB 다(SPEC.md §14.2 S6).
pub const BUDGET_RSS_BYTES: usize = 32 * 1024 * 1024;

/// 한 유닛의 예산. 어기면 패닉한다 — 벤치가 곧 게이트다.
pub fn assert_within_budget(r: &Report) {
    let u = &r.unit;
    let floor = r.demand_mb_s * BUDGET_FEED_OF_DEMAND;
    assert!(
        r.feed_mb_s >= floor,
        "{u}: feed {:.1} MB/s < 수요 {:.1} MB/s. 이 미러는 **데몬이 tee 로 배달하는 속도보다 \
         느리다** — 앱이 닫힌 채(분리 모드) 세션이 폭주하면 데몬은 이 구독자의 바이트를 떨구고, \
         복원 화면에 구멍이 남는다. 실측으로 확인된 손실이다(SPEC.md §14.3).",
        r.feed_mb_s,
        r.demand_mb_s
    );
    assert!(
        r.rehydrate_ms <= BUDGET_PAINT_MS,
        "{u}: rehydrate {:.2} ms > 예산 {BUDGET_PAINT_MS} ms (직렬화기 퇴행)",
        r.rehydrate_ms
    );
    assert!(
        r.paint_bytes <= BUDGET_PAINT_BYTES,
        "{u}: 페인트 {} B > 예산 {BUDGET_PAINT_BYTES} B",
        r.paint_bytes
    );
    assert!(
        r.cold_ms <= BUDGET_PAINT_MS,
        "{u}: cold {:.2} ms > 예산 {BUDGET_PAINT_MS} ms (직렬화기 퇴행)",
        r.cold_ms
    );
    assert!(
        r.cold_bytes <= BUDGET_PAINT_BYTES,
        "{u}: 봉인 {} B > 예산 {BUDGET_PAINT_BYTES} B",
        r.cold_bytes
    );
    assert!(
        r.rss_bytes <= BUDGET_RSS_BYTES,
        "{u}: rss {} B > 예산 {BUDGET_RSS_BYTES} B",
        r.rss_bytes
    );
}

