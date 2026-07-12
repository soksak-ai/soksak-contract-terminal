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

    Report {
        unit: unit.to_string(),
        feed_mb_s: median(feed),
        rehydrate_ms: median(rehydrate),
        paint_bytes,
        cold_ms: median(cold),
        cold_bytes,
        live_bytes: live,
        rss_bytes: rss,
    }
}

// ── 보고 — 한 줄 직렬화(유닛이 쓰고, 표가 읽는다) + 표 ─────────────────────────

impl Report {
    pub fn to_line(&self) -> String {
        format!(
            "{} {} {} {} {} {} {} {}",
            self.unit,
            self.feed_mb_s,
            self.rehydrate_ms,
            self.paint_bytes,
            self.cold_ms,
            self.cold_bytes,
            self.live_bytes,
            self.rss_bytes
        )
    }

    pub fn from_line(s: &str) -> Result<Report, String> {
        let f: Vec<&str> = s.split_whitespace().collect();
        if f.len() != 8 {
            return Err(format!("bench line has {} fields, want 8", f.len()));
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
        })
    }
}

/// 사람이 읽는 비교표. 축마다 최고값을 기준으로 상대 배수를 함께 찍는다 — 절대값만으로는
/// "이 엔진이 저 엔진보다 얼마나 무거운가"가 눈에 안 들어온다.
pub fn table(reports: &[Report]) -> String {
    let mut out = String::new();
    out.push_str(&format!("corpus: {}\n", corpus_shape()));
    out.push_str(&format!("repeats: {REPEATS} (median), release build\n\n"));
    out.push_str(&format!(
        "{:<12} {:>11} {:>10} {:>10} {:>9} {:>9} {:>11} {:>10}\n",
        "unit", "feed MB/s", "rehyd ms", "paint KB", "cold ms", "cold KB", "heap MB", "rss MB"
    ));
    out.push_str(&"-".repeat(88));
    out.push('\n');

    let best_feed = reports.iter().map(|r| r.feed_mb_s).fold(0.0_f64, f64::max);
    let least_rss = reports.iter().map(|r| r.rss_bytes).min().unwrap_or(1).max(1);

    for r in reports {
        out.push_str(&format!(
            "{:<12} {:>9.1}{:<2} {:>10.2} {:>10.1} {:>9.2} {:>9.1} {:>11.1} {:>8.1}{:<2}\n",
            r.unit,
            r.feed_mb_s,
            if r.feed_mb_s >= best_feed * 0.999 { " ★" } else { "" },
            r.rehydrate_ms,
            r.paint_bytes as f64 / 1024.0,
            r.cold_ms,
            r.cold_bytes as f64 / 1024.0,
            r.live_bytes as f64 / 1e6,
            r.rss_bytes as f64 / 1e6,
            if r.rss_bytes <= (least_rss as f64 * 1.05) as usize { " ★" } else { "" },
        ));
    }
    out
}
