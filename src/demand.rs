//! 수요 — 미러가 **따라잡아야 하는 상대**를 잰다. 성능 예산은 여기서 나온다.
//!
//! 예산을 후보(엔진 유닛)의 실측 분포에서 역산하면 그 예산은 후보가 정한 것이 된다 — 전 후보가
//! 함께 느려져도 예산이 따라 내려가고, 아무도 못 잡는다. 그래서 예산은 **요구**에서 유도한다.
//! 이 계약의 성능 요구는 하나다:
//!
//!   **미러는 tee gap 의 원인이 되어서는 안 된다.**
//!
//! gap 은 사이드카가 데몬의 tee 를 배달 속도보다 느리게 소비할 때 난다(SPEC.md §6.2). 아무리
//! 깊은 큐도 **지속적으로** 느린 소비자를 구하지 못한다 — 홍수가 충분히 길면 반드시 넘친다.
//! 그러므로 요구는 "미러는 자기를 먹여 주는 관보다 빨라야 한다"로 정확히 환산된다.
//!
//! ## 두 숫자
//!
//! **① PTY 천장**([`pty_ceiling_mb_s`]) — OS 가 PTY 로 바이트를 나를 수 있는 최대 속도. 어떤
//! 생산자도 이보다 빠를 수 없다(빌드 로그든 `yes` 든, 찍을 내용을 **계산**해야 하는 생산자는
//! 이미 만들어 둔 바이트를 쏟기만 하는 `cat` 을 이길 수 없다). 생산의 절대 상한이다.
//!
//! **② tee 배달률**([`tee_delivery_mb_s`]) — **이것이 수요다.** 미러가 먹는 것은 PTY 가 아니라
//! tee 이고, 바이트는 미러에 닿기 전에 §6.2 가 규정한 관을 통과한다: PTY read → 링 적재 →
//! 길이-접두 프레임 → 유닉스 소켓 → 사이드카 read. 그 관이 낼 수 있는 최대가 미러가 마주할
//! 최대다. 관을 여기서 그대로 세워 잰다 — 데몬 크레이트를 링크하지 않고(SPEC.md §6 이 이미
//! "문서화된 배선을 구현한다"고 정한 그대로), 코어에 의존하지 않고, 엔진도 VT 도 모르는 채로.
//!
//! 두 숫자를 함께 내는 이유는 ①이 ②의 상한임을 눈으로 확인시키기 위해서다. 판정에 쓰는 것은
//! ②다 — 미러의 상대는 커널이 아니라 관이다.

use std::io::{Read, Write};
use std::time::Instant;

/// 천장 측정에 쏟는 바이트 수 — 파이프 버퍼·캐시 워밍이 묻히기에 충분하고, 몇 초 안에 끝난다.
const FLOOD_BYTES: usize = 64 * 1024 * 1024;

/// 데몬이 PTY 를 읽는 버퍼 크기. 관의 알갱이는 여기서 정해진다 — 한 번 읽은 만큼이 tee 프레임
/// 하나가 되고, 프레임마다 syscall 이 든다. 모델이 이 값을 키우면 실제보다 빠른 관을 재게 되고,
/// 그 위에 세운 예산은 우리 시스템의 요구가 아니게 된다.
const DAEMON_READ_BUF: usize = 8192;

/// 세션 원시 링 용량. 데몬은 읽은 바이트를 모두 여기 한 번 적재한다.
const RING_CAP: usize = 256 * 1024;

/// 반복 횟수(중앙값). 커널 경로는 흔들림이 작아 다섯이면 충분하다.
const REPEATS: usize = 5;

/// 실 PTY 가 바이트를 나를 수 있는 최대 속도(MB/s, 중앙값) — **수요의 천장**.
///
/// 측정 조건이 곧 규정이다: release 빌드, 다른 부하 없는 기계, [`REPEATS`]회 중앙값.
/// 재보정은 이 함수를 다시 돌리는 것이지, 예산을 낮추는 것이 아니다.
pub fn pty_ceiling_mb_s() -> f64 {
    let payload = flood_file();
    let mut runs: Vec<f64> = Vec::with_capacity(REPEATS);
    for _ in 0..REPEATS {
        runs.push(one_flood(&payload));
    }
    runs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    runs[runs.len() / 2]
}

/// **수요** — §6.2 의 tee 관이 바이트를 미러 앞까지 배달할 수 있는 최대 속도(MB/s, 중앙값).
///
/// 미러가 이보다 느리면, 충분히 긴 홍수에서 데몬은 반드시 이 구독자를 떨군다(gap). 그래서 이
/// 숫자가 곧 feed 예산이다 — 여유분도, 눈대중 계수도 없다. 상대는 후보가 아니라 관이다.
pub fn tee_delivery_mb_s() -> f64 {
    let payload = flood_file();
    let mut runs: Vec<f64> = (0..REPEATS).map(|_| one_tee_flood(&payload)).collect();
    runs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    runs[runs.len() / 2]
}

/// 한 번의 tee 홍수 — PTY 에 쏟고, 데몬이 하는 일(링 적재 + 프레임 + 소켓 쓰기)을 그대로 한 뒤,
/// 구독자 끝에서 읽히는 속도를 잰다. 시간은 **구독자가 EOF 를 볼 때까지**를 잰다 — 미러가
/// 마주하는 것이 바로 그 끝이기 때문이다.
fn one_tee_flood(payload: &std::path::Path) -> f64 {
    let master = spawn_cat_on_pty(payload);

    // §6.2 의 스트림 소켓. 데몬 쪽(fan-out)과 구독자 쪽(미러)이 마주 본다.
    let mut fds = [0 as libc::c_int; 2];
    // SAFETY: socketpair 는 fd 두 개를 out-param 으로 채운다.
    let rc = unsafe { libc::socketpair(libc::AF_UNIX, libc::SOCK_STREAM, 0, fds.as_mut_ptr()) };
    assert_eq!(rc, 0, "socketpair 실패: {}", std::io::Error::last_os_error());
    let (daemon_end, mirror_end) = (fd_file(fds[0]), fd_file(fds[1]));

    let t = Instant::now();

    // 데몬 — PTY 를 읽어 링에 적재하고, 길이-접두 프레임으로 구독자에게 복제한다.
    let pump = std::thread::spawn(move || {
        let mut master = master;
        let mut sock = daemon_end;
        // 세션 링(soksak-ptyd 와 같은 용량). 데몬은 모든 바이트를 여기 한 번 적재한다.
        let mut ring = vec![0u8; RING_CAP];
        let mut head = 0usize;
        // 데몬의 읽기 버퍼와 같은 크기 — 관의 프레임 알갱이가 여기서 정해진다(작을수록 syscall 이
        // 잦고 관이 느리다). 모델이 실제 배선과 다르면 그 모델이 낸 수요는 우리 관의 수요가 아니다.
        let mut buf = vec![0u8; DAEMON_READ_BUF];
        loop {
            let n = match master.read(&mut buf) {
                Ok(0) => break,
                Err(e) if e.raw_os_error() == Some(libc::EIO) => break,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Ok(n) => n,
                Err(e) => panic!("PTY master 읽기 실패: {e}"),
            };
            // 링 적재(데몬이 바이트마다 치르는 비용).
            let end = (head + n) % RING_CAP;
            if head + n <= RING_CAP {
                ring[head..head + n].copy_from_slice(&buf[..n]);
            } else {
                let split = RING_CAP - head;
                ring[head..].copy_from_slice(&buf[..split]);
                ring[..end].copy_from_slice(&buf[split..n]);
            }
            head = end;
            // tee 프레임: [kind: u8][len: u32 BE][payload].
            let mut frame = Vec::with_capacity(5 + n);
            frame.push(0u8); // TEE_FRAME_DATA
            frame.extend_from_slice(&(n as u32).to_be_bytes());
            frame.extend_from_slice(&buf[..n]);
            if sock.write_all(&frame).is_err() {
                break; // 구독자가 사라졌다.
            }
        }
        std::hint::black_box(&ring);
    });

    // 구독자(미러 자리) — 프레임을 읽어 소비한다. 해석은 하지 않는다: 관의 배달 속도를 재는 것이지
    // 엔진을 재는 것이 아니다.
    let mut sock = mirror_end;
    let mut hdr = [0u8; 5];
    let mut total = 0usize;
    let mut payload_buf = vec![0u8; 1 << 20];
    loop {
        if sock.read_exact(&mut hdr).is_err() {
            break; // EOF — 데몬이 관을 닫았다.
        }
        let len = u32::from_be_bytes([hdr[1], hdr[2], hdr[3], hdr[4]]) as usize;
        if payload_buf.len() < len {
            payload_buf.resize(len, 0);
        }
        sock.read_exact(&mut payload_buf[..len]).expect("tee 프레임 payload");
        std::hint::black_box(&payload_buf[..len]);
        total += len;
    }
    let secs = t.elapsed().as_secs_f64();
    pump.join().expect("tee pump");

    assert!(total > FLOOD_BYTES / 2, "tee 가 배달한 바이트가 너무 적다: {total}");
    (total as f64 / 1e6) / secs
}

/// 쏟을 파일 — 한 번 만들고 재사용한다(멱등). 내용은 상관없다: PTY 는 바이트를 해석하지 않는다.
fn flood_file() -> std::path::PathBuf {
    let path = std::env::temp_dir().join("soksak-contract-terminal-pty-flood.bin");
    let fresh = std::fs::metadata(&path).map(|m| m.len() as usize == FLOOD_BYTES).unwrap_or(false);
    if !fresh {
        let chunk = vec![b'x'; 1 << 20];
        let mut out = Vec::with_capacity(FLOOD_BYTES);
        while out.len() < FLOOD_BYTES {
            out.extend_from_slice(&chunk);
        }
        std::fs::write(&path, &out).expect("PTY 홍수 파일을 쓸 수 없다");
    }
    path
}

/// 한 번의 홍수 — PTY 에 `cat` 을 앉히고, master 가 읽어 낸 속도를 잰다. 읽기만 한다(해석 0):
/// 이것이 커널이 낼 수 있는 최대다.
fn one_flood(payload: &std::path::Path) -> f64 {
    let mut master = spawn_cat_on_pty(payload);
    let mut buf = vec![0u8; 1 << 16];
    let mut total = 0usize;
    let t = Instant::now();
    loop {
        match master.read(&mut buf) {
            Ok(0) => break,
            // 슬레이브가 닫히면 master read 는 EIO 로 EOF 를 알린다(리눅스·macOS 공통).
            Err(e) if e.raw_os_error() == Some(libc::EIO) => break,
            Ok(n) => total += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => panic!("PTY master 읽기 실패: {e}"),
        }
    }
    let secs = t.elapsed().as_secs_f64();
    assert!(total > FLOOD_BYTES / 2, "PTY 가 쏟은 바이트가 너무 적다: {total}");
    (total as f64 / 1e6) / secs
}

/// PTY 에 `cat <payload>` 를 앉히고 master 를 돌려준다. 자식은 SIGCHLD 로 거둔다(무시 설정으로
/// 커널이 거둔다 — 이 프로세스는 시험 하니스이지 프로세스 관리자가 아니다).
fn spawn_cat_on_pty(payload: &std::path::Path) -> std::fs::File {
    // SAFETY: 자식을 거두지 않고 두면 좀비가 된다. SIG_IGN 이면 커널이 자동으로 거둔다.
    unsafe { libc::signal(libc::SIGCHLD, libc::SIG_IGN) };

    let mut master: libc::c_int = -1;
    // SAFETY: forkpty 는 master fd 를 out-param 으로 채운다. 자식(0)은 exec 로 즉시 대체된다.
    let pid = unsafe {
        libc::forkpty(&mut master, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut())
    };
    assert!(pid >= 0, "forkpty 실패: {}", std::io::Error::last_os_error());

    if pid == 0 {
        // 자식 — PTY 슬레이브가 stdout 이다. cat 이 파일을 그대로 쏟는다.
        let cat = std::ffi::CString::new("/bin/cat").unwrap();
        let arg = std::ffi::CString::new(payload.to_str().expect("경로")).unwrap();
        let argv = [cat.as_ptr(), arg.as_ptr(), std::ptr::null()];
        // SAFETY: exec 는 돌아오지 않는다. 돌아왔다면 실패이므로 즉시 죽는다.
        unsafe {
            libc::execv(cat.as_ptr(), argv.as_ptr());
            libc::_exit(127);
        }
    }
    fd_file(master)
}

/// raw fd 의 소유권을 File 로 넘긴다(Drop 이 닫는다).
fn fd_file(fd: libc::c_int) -> std::fs::File {
    // SAFETY: fd 는 방금 우리가 만든 것이고, 여기서 유일한 소유자가 된다.
    unsafe { <std::fs::File as std::os::fd::FromRawFd>::from_raw_fd(fd) }
}
