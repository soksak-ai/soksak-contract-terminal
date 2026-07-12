#!/bin/bash
# 프론트 터미널의 **바이트 소비율** — 성능 예산의 수요측 근거(SPEC.md §14.1).
#
# 왜 이걸 재는가: 데몬은 프론트가 HIGH_WATERMARK 만큼 밀리면 PTY 읽기를 멈춘다(soksak-ptyd).
# 그래서 바이트 강의 **속도를 정하는 것은 프론트 터미널**이지 커널도 미러도 아니다. 미러가
# 프론트보다 빠르면 미러는 결코 tee gap 의 원인이 될 수 없다 — 그것이 요구이고, 이 숫자가 그
# 요구의 크기다.
#
# 무엇을 재는가: 우리가 싣는 프론트(@xterm/headless — 화면을 그리는 부분을 뺀, xterm.js 의 파서와
# 버퍼 그대로)에 계약의 **같은 코퍼스**를 먹인다. 렌더러가 빠졌으므로 이 숫자는 실제 프론트의
# 소비율보다 **빠르다** — 즉 수요의 상한이다. 상한으로 예산을 세우는 것이 안전한 방향이다.
#
# 이 스크립트는 게이트가 아니다(게이트는 node 를 요구하지 않는다). 예산을 **재보정할 때** 돌린다.
set -euo pipefail
export PATH="$HOME/.cargo/bin:$PATH"

HERE="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$HERE/target/frontend-demand"
mkdir -p "$WORK"

echo "== 계약 코퍼스를 꺼낸다(프론트와 미러가 같은 것을 먹어야 비교가 된다)"
SOKSAK_CORPUS_OUT="$WORK/corpus.bin" \
  cargo test --release --test demand -- --ignored dump_corpus --nocapture 2>/dev/null | grep bytes

echo "== 프론트 파서를 세운다(@xterm/headless — 우리가 싣는 xterm.js 의 파서·버퍼)"
cd "$WORK"
[ -f package.json ] || echo '{"name":"frontend-demand","private":true}' > package.json
[ -d node_modules/@xterm/headless ] || npm install --silent --no-audit --no-fund @xterm/headless@^6

cat > measure.mjs <<'JS'
import xterm from '@xterm/headless'; // CJS 번들이라 default 로 받는다
const { Terminal } = xterm;
import { readFileSync } from 'node:fs';

const bytes = readFileSync(new URL('./corpus.bin', import.meta.url));
const mb = bytes.length / 1e6;
const REPEATS = 5;
const runs = [];

for (let i = 0; i < REPEATS; i++) {
  // 미러와 같은 격자(계약이 고정한다: 80x24). scrollback 도 미러의 복원 창과 같게 준다.
  const term = new Terminal({ cols: 80, rows: 24, scrollback: 1000, allowProposedApi: true });
  const t0 = process.hrtime.bigint();
  // write 는 비동기 청크 처리다 — 콜백이 곧 "이 바이트를 다 소비했다"는 프론트의 ack 이고,
  // 데몬의 플로우 제어가 기다리는 것이 정확히 그 ack 이다.
  await new Promise((done) => term.write(bytes, done));
  const secs = Number(process.hrtime.bigint() - t0) / 1e9;
  runs.push(mb / secs);
  term.dispose();
}

runs.sort((a, b) => a - b);
const median = runs[runs.length >> 1];
console.log(`frontend (xterm.js headless parser) ${median.toFixed(1)} MB/s  (median of ${REPEATS}, ${mb.toFixed(2)} MB corpus)`);
JS

node measure.mjs
