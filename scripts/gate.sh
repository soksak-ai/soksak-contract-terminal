#!/bin/bash
# 함대 게이트 — 계약이 규율하는 **모든 엔진 유닛**을 한 번에 판정한다. 이것이 합격 판정의
# 정본 경로다(유닛 하나만 보는 scripts/gate.sh 는 그 유닛의 몫만 본다).
#
# 여기서만 볼 수 있는 것이 하나 있다: feed 의 **상대 가드**(같은 실행에서 최고 유닛의 ¼ 미만이면
# 불합격, SPEC.md §14.2). 유닛 혼자서는 자기가 느려진 것인지 기계가 느린 것인지 알 수 없다 —
# 나란히 놓아야 보인다. 그래서 상대 가드는 유닛이 아니라 이 스크립트가 강제한다.
#
# 사용: scripts/gate.sh [<유닛 repo 들이 있는 디렉토리>]
set -euo pipefail
export PATH="$HOME/.cargo/bin:$PATH"

HERE="$(cd "$(dirname "$0")/.." && pwd)"
SIDECARS="${1:-$HOME/.soksak-dev/sidecars}"
UNITS=(alacritty wezterm vt100 ghostty)

BENCH_OUT="$(mktemp -d)"
trap 'rm -rf "$BENCH_OUT"' EXIT

echo "== 계약 자체 시험(코덱 왕복 등)"
( cd "$HERE" && cargo test --release )

for u in "${UNITS[@]}"; do
  GATE="$SIDECARS/soksak-sidecar-terminal-$u/scripts/gate.sh"
  if [ ! -x "$GATE" ]; then
    echo "유닛 게이트가 없다: $GATE" >&2
    exit 1
  fi
  "$GATE" "$BENCH_OUT"
done

echo "== 함대: 상대 가드 + 비교표(SPEC.md §14.2)"
( cd "$HERE" && SOKSAK_BENCH_OUT="$BENCH_OUT" \
    cargo test --release --test bench_table -- --ignored --nocapture )

echo "== FLEET GATE PASS"
