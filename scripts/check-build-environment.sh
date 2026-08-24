#!/bin/sh
set -eu

[ "$#" -eq 0 ] || { echo 'BUILD_DECLARATION_INVALID: usage: check-build-environment.sh' >&2; exit 78; }
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
rust_expected=$(sed -n 's/^channel = "\([^"]*\)"$/\1/p' "$root/rust-toolchain.toml")
rust_actual=$(rustc --version 2>/dev/null | awk '{print $2}' || true)
rust_host=$(rustc -vV 2>/dev/null | sed -n 's/^host: //p' || true)

case "$(uname -s)-$(uname -m)" in
  Darwin-arm64) required_host=aarch64-apple-darwin ;;
  Darwin-x86_64) if [ "$(sysctl -n hw.optional.arm64 2>/dev/null || true)" = 1 ]; then required_host=aarch64-apple-darwin; else required_host=x86_64-apple-darwin; fi ;;
  Linux-aarch64|Linux-arm64) required_host=aarch64-unknown-linux-gnu ;;
  Linux-x86_64) required_host=x86_64-unknown-linux-gnu ;;
  MINGW*-x86_64|MSYS*-x86_64|CYGWIN*-x86_64) required_host=x86_64-pc-windows-msvc ;;
  *) echo 'TOOLCHAIN_MISMATCH: unsupported host' >&2; exit 78 ;;
esac

if [ -z "$rust_expected" ] || [ "$rust_actual" != "$rust_expected" ] || [ "$rust_host" != "$required_host" ]; then
  printf 'TOOLCHAIN_MISMATCH: expected rust=%s host=%s; actual rust=%s host=%s\n' \
    "${rust_expected:-missing}" "$required_host" "${rust_actual:-missing}" "${rust_host:-unknown}" >&2
  exit 78
fi
printf 'BUILD_ENVIRONMENT_READY rust=%s host=%s\n' "$rust_actual" "$rust_host"
