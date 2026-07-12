//! 실 데몬 수요 — **모델이 아니라 진짜 `soksak-ptyd`** 를 세워, tee 에 실제로 도착하는 지속
//! 속도를 잰다. 예산의 전제가 사실인지 여기서 판가름 난다.
//!
//! 한때 이 자리에 §6.2 의 배선을 흉내 낸 **모델**이 있었다. 같은 syscall, 같은 프레이밍, 같은
//! 링 — 그런데 190 MB/s 를 냈다. 실제 데몬은 그것의 **0.4배**로 배달한다(뮤텍스·프레임 큐·notify·
//! 별도 writer 스레드를 모델은 치르지 않았다). 잴 수 있는 것을 그럴듯하게 흉내 내는 모델은 측정을
//! 건너뛰라는 초대장이다. 그래서 모델은 지웠고, 여기서는 진짜를 세운다.
//!
//! ## 두 모드
//!
//! **부착(attached)** — 앱이 살아 있고 프론트 터미널이 세션에 붙어 있다. 데몬은 프론트에
//! 라이브 바이트를 밀어 넣고, 미확인분이 `HIGH_WATERMARK` 를 넘으면 **PTY 읽기를 멈춘다**
//! (프론트의 ack 이 `LOW_WATERMARK` 아래로 내려야 재개). 강의 속도를 프론트가 정한다.
//!
//! **분리(detached)** — 앱이 없다. 셸은 계속 돈다. **미러가 존재하는 이유가 바로 이 모드다.**
//! 그리고 데몬의 정지 조건은 `paused && attached.is_some()` 이다 — 부착이 없으면 **아무것도
//! 읽기를 늦추지 않는다.** tee 는 비차단이고, 느린 구독자는 gap 으로 잃는다.
//!
//! 요구는 둘 중 **더 엄한 쪽**이다. 미러의 존재 이유가 분리 모드인데 그 모드를 빼고 예산을
//! 세우면, 예산은 미러가 가장 필요한 순간을 보지 않는 것이 된다.
//!
//! ## 무엇을 재는가
//!
//! ① **도착률** — 구독자가 최대 속도로 먹을 때 tee 에 도착하는 지속 속도. 이것이 수요다.
//! ② **gap** — 구독자를 실제 미러 속도로 **묶어 두었을 때** 데몬이 떨군 바이트. gap 은 복원
//!    화면의 구멍이므로 사용자 가시 손상이다(SPEC.md §6.2).
//! ③ **꼬리 도착 여부** — 홍수가 끝난 뒤 셸이 찍는 마지막 마커가 구독자에게 닿는가. **화면**을
//!    복원하는 미러에게 중요한 것은 홍수의 중간이 아니라 **끝**이다: 중간은 어차피 스크롤백 창
//!    밖으로 밀려난다. 꼬리를 잃으면 복원 화면이 통째로 낡은 것이 된다.
//!
//! 코어 크레이트를 링크하지 않는다. 문서화된 배선(§6.1 NDJSON control, §6.2 길이-접두 tee)을
//! 그대로 구현한다 — 유닛이 그러듯이. 데몬 **바이너리**만 `SOKSAK_PTYD_BIN` 으로 주입받는다.

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime};

/// 홍수 크기 — 링(256 KiB)과 tee 버퍼(1 MB)를 여러 자릿수 넘겨야 "지속" 조건이 성립한다.
const FLOOD_BYTES: usize = 64 * 1024 * 1024;

/// 홍수가 끝난 뒤 셸이 찍는 마커. 이것이 도착하지 않으면 복원 화면은 낡은 것이다.
const TAIL_MARK: &str = "TAIL-MARK-9f3c";

/// 한 번의 측정 결과.
#[derive(Debug, Clone)]
pub struct Arrival {
    pub mode: &'static str,
    /// 지속 도착률(MB/s) — 구독자가 최대 속도로 먹을 때. **이것이 수요다.**
    pub arrival_mb_s: f64,
    pub data_bytes: u64,
    /// 데몬이 이 구독자에게서 떨군 바이트(gap 마커의 합).
    pub gap_bytes: u64,
    /// 홍수 뒤의 마지막 마커가 도착했는가.
    pub tail_seen: bool,
}

/// **수요**(MB/s, 중앙값) — 분리 모드에서 데몬이 tee 로 배달하는 지속 속도. 예산이 곧 이 값이다.
///
/// 한 번의 측정은 커널·스케줄러 때문에 10% 남짓 흔들린다. 예산은 그 흔들림 위에 서면 안 되므로
/// 중앙값을 쓴다(측정 조건 규정 — SPEC.md §14.1).
pub fn detached_arrival_mb_s(bin: &Path) -> f64 {
    let mut runs: Vec<f64> = (0..3).map(|_| measure(bin, false, None).arrival_mb_s).collect();
    runs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    runs[runs.len() / 2]
}

/// 데몬 바이너리 — 유닛 게이트가 코어에서 빌드해 주입한다.
pub fn ptyd_bin() -> Option<PathBuf> {
    match std::env::var("SOKSAK_PTYD_BIN") {
        Ok(p) if !p.is_empty() && Path::new(&p).exists() => Some(PathBuf::from(p)),
        _ => None,
    }
}

/// 한 모드의 도착률·gap·꼬리를 잰다.
///
/// `consumer_mb_s = None` 이면 구독자가 최대 속도로 먹는다 → **도착률**을 얻는다.
/// `Some(rate)` 면 구독자를 그 속도로 묶는다 → 그 속도의 미러가 겪을 **gap 과 꼬리 손실**을 얻는다.
pub fn measure(bin: &Path, attached: bool, consumer_mb_s: Option<f64>) -> Arrival {
    let home = fresh_home();
    let flood = flood_script(&home);
    let _daemon = Daemon::start(bin, &home);
    let token = read_token(&home);

    // §6.1 control — 세션을 띄운다. shell 은 홍수 스크립트 자신이다(입력을 보낼 필요가 없다).
    let mut control = Ndjson::connect(&control_socket(&home), &token, None).expect("control");
    let reply = control.request(&format!(
        r#"{{"op":"createOrAttach","paneId":"p1","cols":80,"rows":24,"cwd":null,"shell":"{}","env":[["TERM","xterm-256color"]],"envRemove":[],"windowLabel":"w-demand"}}"#,
        flood.display()
    ));
    let session = json_u64(&reply, "session").expect("session id");

    // §6.2 tee — 홍수가 시작되기 전에 구독한다.
    let mut tee = TeeSub::subscribe(&stream_socket(&home), &token, session);

    // 부착 모드면 프론트 터미널 자리를 채운다: 라이브 스트림을 읽고 ack 한다. 이 ack 이
    // 데몬의 읽기 루프를 늦추는 유일한 브레이크다.
    let _front = if attached { Some(FrontEnd::attach(&home, &token, session)) } else { None };

    let t = Instant::now();
    let mut data_bytes = 0u64;
    let mut gap_bytes = 0u64;
    let mut tail_seen = false;
    // 꼬리 마커는 프레임 경계에 걸린다 — 직전 프레임의 꼬리를 이어 붙여 찾는다. 바이트 그대로
    // 훑는다(프레임마다 UTF-8 로 옮기고 스캔하면 그 비용이 도착률에 섞여 든다 — 재는 것은
    // 데몬의 배달이지 구독자의 문자열 처리가 아니다).
    let mark = TAIL_MARK.as_bytes();
    let mut carry: Vec<u8> = Vec::with_capacity(mark.len() * 2);

    loop {
        match tee.next_frame() {
            Some(Frame::Data(bytes)) => {
                data_bytes += bytes.len() as u64;
                if !tail_seen {
                    carry.extend_from_slice(&bytes);
                    tail_seen = carry.windows(mark.len()).any(|w| w == mark);
                    let keep = mark.len().saturating_sub(1);
                    if carry.len() > keep {
                        let cut = carry.len() - keep;
                        carry.drain(..cut);
                    }
                }
                // 구독자를 실제 미러 속도로 묶는다(느린 미러가 겪는 것을 그대로 겪게).
                //
                // 프레임마다 sleep 하면 안 된다: 8 KB 프레임을 70 MB/s 로 먹는 데 드는 시간은
                // 117 µs 인데 스레드 sleep 의 입도는 그보다 훨씬 굵어서, 실제로는 8 MB/s 짜리
                // 구독자를 만들어 놓고 70 MB/s 라고 부르게 된다. 그러니 **누적 마감시각**을 쫓는다.
                if let Some(rate) = consumer_mb_s {
                    let due = Duration::from_secs_f64((data_bytes as f64 / 1e6) / rate);
                    let now = t.elapsed();
                    if due > now {
                        std::thread::sleep(due - now);
                    }
                }
                if tail_seen {
                    break;
                }
            }
            Some(Frame::Gap(from, to)) => gap_bytes += to - from,
            None => break, // EOF — 셸이 끝나고 데몬이 관을 닫았다.
        }
    }
    let secs = t.elapsed().as_secs_f64();

    Arrival {
        mode: if attached { "attached" } else { "detached" },
        arrival_mb_s: (data_bytes as f64 / 1e6) / secs,
        data_bytes,
        gap_bytes,
        tail_seen,
    }
}

// ── 홍수 셸 ──────────────────────────────────────────────────────────────────

/// 세션이 실행할 프로그램: 큰 파일을 쏟고, 끝에 마커를 찍는다. 마커가 도착하는지가 ③의 답이다.
fn flood_script(home: &Path) -> PathBuf {
    let payload = home.join("flood.bin");
    let chunk = vec![b'x'; 1 << 20];
    let mut out = Vec::with_capacity(FLOOD_BYTES);
    while out.len() < FLOOD_BYTES {
        out.extend_from_slice(&chunk);
    }
    std::fs::write(&payload, &out).expect("flood payload");

    let script = home.join("flood.sh");
    std::fs::write(
        &script,
        format!("#!/bin/sh\ncat '{}'\nprintf '\\n{TAIL_MARK}\\n'\nsleep 1\n", payload.display()),
    )
    .expect("flood script");
    let mut perm = std::fs::metadata(&script).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut perm, 0o755);
    std::fs::set_permissions(&script, perm).unwrap();
    script
}

// ── 데몬 기동 ────────────────────────────────────────────────────────────────

struct Daemon {
    child: Child,
    home: PathBuf,
}

impl Daemon {
    fn start(bin: &Path, home: &Path) -> Self {
        let child = Command::new(bin)
            .env("SOKSAK_HOME", home)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn soksak-ptyd");
        let ctrl = control_socket(home);
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(5) {
            if UnixStream::connect(&ctrl).is_ok() && token_path(home).exists() {
                return Daemon { child, home: home.to_path_buf() };
            }
            std::thread::sleep(Duration::from_millis(30));
        }
        panic!("ptyd control socket did not come up");
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.home);
    }
}

fn fresh_home() -> PathBuf {
    let nanos = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_nanos();
    let home = PathBuf::from(std::env::var("HOME").unwrap())
        .join(".soksak-e2e")
        .join(format!("contract-demand-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&home).expect("home");
    home
}

// 경로 규약(§4·§6.1) — 프로토콜 버전으로 키잉된다. 코어 크레이트를 링크하지 않으므로 규약을 옮긴다.
const PTYD_PROTOCOL_VERSION: u32 = 1;
fn run_dir(home: &Path) -> PathBuf {
    home.join("run")
}
fn control_socket(home: &Path) -> PathBuf {
    run_dir(home).join(format!("ptyd-p{PTYD_PROTOCOL_VERSION}.sock"))
}
fn stream_socket(home: &Path) -> PathBuf {
    run_dir(home).join(format!("ptyd-p{PTYD_PROTOCOL_VERSION}-stream.sock"))
}
fn token_path(home: &Path) -> PathBuf {
    run_dir(home).join(format!("ptyd-p{PTYD_PROTOCOL_VERSION}.token"))
}
fn read_token(home: &Path) -> String {
    std::fs::read_to_string(token_path(home)).expect("token").trim().to_string()
}

// ── §6.1 NDJSON control ──────────────────────────────────────────────────────

struct Ndjson {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
}

impl Ndjson {
    fn connect(path: &Path, token: &str, hello_extra: Option<&str>) -> Option<Self> {
        let s = UnixStream::connect(path).ok()?;
        let mut me = Ndjson { reader: BufReader::new(s.try_clone().ok()?), writer: s };
        let extra = hello_extra.unwrap_or("");
        let hello = format!(
            r#"{{"version":{PTYD_PROTOCOL_VERSION},"token":"{token}","clientId":"contract-demand"{extra}}}"#
        );
        me.send(&hello);
        let ack = me.recv();
        assert!(ack.contains("\"ok\":true"), "hello refused: {ack}");
        Some(me)
    }

    fn send(&mut self, line: &str) {
        writeln!(self.writer, "{line}").expect("write");
        self.writer.flush().expect("flush");
    }

    fn recv(&mut self) -> String {
        let mut line = String::new();
        self.reader.read_line(&mut line).expect("read");
        line
    }

    fn request(&mut self, body: &str) -> String {
        self.send(body);
        let r = self.recv();
        assert!(r.contains("\"ok\":true"), "request failed: {body} -> {r}");
        r
    }
}

/// 답에서 숫자 하나를 꺼낸다. JSON 라이브러리를 들이지 않는다 — 이 하니스가 읽는 숫자는 둘뿐이고
/// (session, startSeq), 둘 다 답의 유일한 그 이름이다.
fn json_u64(json: &str, key: &str) -> Option<u64> {
    let pat = format!("\"{key}\":");
    let i = json.find(&pat)? + pat.len();
    let rest = &json[i..];
    let end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
    rest[..end].parse().ok()
}

// ── §6.2 tee 구독 ────────────────────────────────────────────────────────────

enum Frame {
    Data(Vec<u8>),
    Gap(u64, u64),
}

struct TeeSub {
    stream: UnixStream,
}

impl TeeSub {
    fn subscribe(path: &Path, token: &str, session: u64) -> Self {
        let extra = format!(r#","session":{session},"subscribe":true"#);
        let n = Ndjson::connect(path, token, Some(&extra)).expect("tee subscribe");
        TeeSub { stream: n.reader.into_inner() }
    }

    fn next_frame(&mut self) -> Option<Frame> {
        let mut hdr = [0u8; 5];
        self.stream.read_exact(&mut hdr).ok()?;
        let len = u32::from_be_bytes([hdr[1], hdr[2], hdr[3], hdr[4]]) as usize;
        let mut payload = vec![0u8; len];
        self.stream.read_exact(&mut payload).ok()?;
        match hdr[0] {
            0 => Some(Frame::Data(payload)),
            1 => {
                let s = String::from_utf8_lossy(&payload).to_string();
                Some(Frame::Gap(json_u64(&s, "fromSeq")?, json_u64(&s, "toSeq")?))
            }
            k => panic!("모르는 tee 프레임 종류: {k}"),
        }
    }
}

// ── 프론트 터미널 자리 — 부착 모드의 브레이크 ─────────────────────────────────

/// 라이브 attach 를 읽고 ack 한다. 데몬은 미확인분이 HIGH_WATERMARK 를 넘으면 PTY 읽기를 멈추고,
/// ack 이 LOW_WATERMARK 아래로 내려야 재개한다 — **부착 모드에서 강의 속도를 정하는 것이 이것이다.**
/// 여기서는 프론트가 낼 수 있는 **최대**로 ack 한다(파싱도 렌더도 하지 않는다). 그러므로 이 모드의
/// 도착률은 실제 프론트가 만들 수 있는 도착률의 **상한**이다.
struct FrontEnd {
    _handle: std::thread::JoinHandle<()>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl FrontEnd {
    fn attach(home: &Path, token: &str, session: u64) -> Self {
        let extra = format!(r#","session":{session},"subscribe":false,"fromSeq":0"#);
        let n = Ndjson::connect(&stream_socket(home), token, Some(&extra)).expect("attach");
        let mut live = n.reader.into_inner();
        let control = Ndjson::connect(&control_socket(home), token, None).expect("ack control");

        // ack 는 **비동기로** 보낸다. 응답을 기다리면 교착한다: 데몬의 읽기 루프는 세션 뮤텍스를
        // 쥔 채 attach 소켓에 write_all 하고, 그 write 가 뚫리려면 이 스레드가 계속 읽어야 한다.
        // 여기서 ack 응답을 기다리면 이 스레드가 읽기를 멈추고, 데몬은 뮤텍스를 쥔 채 막히고,
        // ack 를 처리할 control 스레드는 그 뮤텍스를 못 얻는다. 실제 앱도 ack 를 기다리지 않는다.
        let mut replies = BufReader::new(control.writer.try_clone().expect("clone"));
        std::thread::spawn(move || {
            let mut line = String::new();
            while replies.read_line(&mut line).map(|n| n > 0).unwrap_or(false) {
                line.clear();
            }
        });
        let mut sender = control.writer;

        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let s = stop.clone();
        let handle = std::thread::spawn(move || {
            let mut buf = vec![0u8; 1 << 16];
            while !s.load(std::sync::atomic::Ordering::Relaxed) {
                match live.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let _ = writeln!(
                            sender,
                            r#"{{"op":"ack","session":{session},"bytes":{n}}}"#
                        );
                        let _ = sender.flush();
                    }
                }
            }
        });
        FrontEnd { _handle: handle, stop }
    }
}

impl Drop for FrontEnd {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}
