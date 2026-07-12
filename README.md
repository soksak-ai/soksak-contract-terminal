# soksak-contract-terminal

**This repo is a contract and its acceptance suite. Nothing here runs.** There is no
binary, no `dist`, no `stage.sh`, no registry entry, and nothing is installed on a
user's machine. Engine units consume it as a **dev-dependency**, and that is the only
way it is consumed.

The repo name is `soksak-contract-terminal`; the contract id is
`soksak-sidecar-terminal-spec@1`. They differ on purpose — the id is a string value
naming the contract a *sidecar unit* implements, and manifests and code keep using it
unchanged. The repo name says what this repo is: a contract, not a sidecar.

## What it holds

- **`SPEC.md`** — the contract. It used to live in the `soksak-sidecar-terminal-alacritty`
  repo, which made one engine unit the owner of the rules every engine unit is judged by.
  It does not any more.
- **`goldens/`** — the declared screens. For each fixture: *this stream must produce this
  screen*, with the reasoning that puts it there at the top of the file.
- **`src/corpus.rs`** — the seven fixture streams. The contract owns them; no unit keeps a
  copy.
- **`src/state.rs`** — the canonical screen state, and with it the rules that decide when
  two screens are the same (SPEC.md §11).
- **`src/lib.rs`** — the `MirrorUnderTest` face a unit implements, and `assert_conforms`,
  which is an assertion function and not a framework.

## There is no judge engine

The suite depends on no VT engine — not even as a dependency. It used to: it rendered
every unit's restore paint with the Alacritty engine and compared that against Alacritty's
rendering of the raw stream. Three things were wrong with it. It defined "correct" as
"what Alacritty does". It made the Alacritty unit its own judge. And it could not see a
real misinterpretation inside another engine, because the re-rendering erased it — which
is not a hypothetical: SPEC.md §13 records the defect it missed and the golden found.

The standard is the declared golden. Every engine, Alacritty included, is an equal
candidate graded against it.

## How a unit is graded

Three axes, each an ordinary assertion:

1. **Interpretation** — feed the corpus stream; the mirror's screen state equals the golden.
2. **Restore** — feed that mirror's `rehydrate` paint to a fresh mirror of the same engine;
   its screen state equals the *same* golden. The golden being external is what stops a
   self-consistent error from hiding.
3. **Replay guard** — no byte leaves the mirror, no query rides in the paint.

A unit's `tests/conformance.rs` implements `MirrorUnderTest` (the one thing it owes: turn
its engine's representation into the canonical form) and calls `assert_conforms` from seven
plain `#[test]` functions.

## Bootstrapping a golden

An engine's output can propose a candidate — `SOKSAK_GOLDEN_OUT=<dir> cargo test --test
conformance -- --ignored dump_goldens`, run from a unit — and comparing the candidates of
several independent engines is a cheap way to find the places worth thinking about. But
agreement is evidence, not authority. A candidate becomes a golden only once it is argued
against the terminal specification, and that argument is written into the file.

## Current standing

| unit | result |
| --- | --- |
| `soksak-sidecar-terminal-alacritty` | 7 / 7 |
| `soksak-sidecar-terminal-vt100` | 7 / 7 (on the fork that adds DEC Special Graphics) |
| `soksak-sidecar-terminal-ghostty` | 7 / 7 |
| `soksak-sidecar-terminal-wezterm` | **6 / 7** — fixture ② `cjk_width` |

The wezterm RED is a real engine defect, not a fixture quirk: given one free column it
packs a double-width character into it instead of wrapping it. SPEC.md §13 has the
reasoning and the verdict.

## Licensing

The contract bundles no engine and depends on none, so it carries no engine's license.
Each unit carries its own.
