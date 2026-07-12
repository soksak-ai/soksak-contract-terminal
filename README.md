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
| `soksak-sidecar-terminal-wezterm` | 7 / 7 (on the fork that makes a wide character obey DECAWM at the margin) |

Both of the engines standing on a fork are there because this suite found a real defect in
them, and both defects were closed at their owner rather than papered over in the unit. The
suite also found a bug in the *mirror's own serializer* — a style left active across a line
break, which bleeds colour on any terminal that erases with the current background. SPEC.md
§13 has all three, with the reasoning.

## Performance is a floor, not a ranking

The suite also measures every unit on one corpus through the same trait — feed throughput, the
cost of the rehydrate paint and the cold checkpoint, and the memory a mirror holds with its
scrollback window full. The budgets in SPEC.md §14 are a floor: a terminal's real output peaks
at a few megabytes per second and the slowest unit consumes the corpus at seventy, so the
differences between the units vanish into the headroom. The table catches an order-of-magnitude
regression; it does not crown anyone.

**The default unit is `soksak-sidecar-terminal-alacritty`, and it is not the fastest one.** The
reason is supply chain: its engine is a published, first-party crate, while two of the others
run on local forks that close defects this suite found, and one runs on a pinned commit of a
library whose authors call its API unstable. Speed does not discriminate between them at the
rates a terminal actually produces. What we depend on does.

## Licensing

The contract bundles no engine and depends on none, so it carries no engine's license.
Each unit carries its own.
