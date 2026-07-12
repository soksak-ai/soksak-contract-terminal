#!/bin/bash
# 함대 게이트 — 계약이 규율하는 **모든 엔진 유닛**을 한 번에 판정한다. 이것이 합격 판정의
# 정본 경로다(유닛 하나만 보는 scripts/gate.sh 는 그 유닛의 몫만 본다).
#
# **이 스크립트는 판정하지 않는다 — 판정은 유닛 게이트에서 이미 끝났다.** 예산은 그 기계의 수요를
# 유닛이 직접 재서 자기와 견주므로(SPEC.md §14.1), 다른 유닛이 무엇을 냈는지 알 필요가 없다.
# 예전에는 여기서 "같은 실행 최고 유닛의 ¼" 이라는 상대 가드를 강제했다. 폐기했다: 후보끼리
# 견주는 판정은 기준을 후보에게 넘긴다. 이 스크립트가 하는 일은 전 유닛의 게이트를 돌리고
# 결과를 한 표로 모으는 것이다.
#
# 계약 자체 시험에는 골든 저작 게이트가 들어 있다(엔진 이름이 골든의 논거에 등장하면 불합격).
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
