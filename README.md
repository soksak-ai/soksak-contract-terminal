# soksak-contract-terminal

**This repo is a contract and its acceptance suite. Nothing here runs.** There is no
binary, no `dist`, no `stage.sh`, no registry entry, and nothing is installed on a
user's machine. Engine units consume it as a **dev-dependency**, and that is the only
way it is consumed.

The repo name is `soksak-contract-terminal`; the contract id is
`soksak-spec-sidecar-terminal`. They differ on purpose — the id is a string value
naming the contract a *sidecar unit* implements, and manifests and code keep using it
unchanged. The repo name says what this repo is: a contract, not a sidecar.

## What it holds

- **`SPEC.md`** — the contract. It used to live in the `soksak-sidecar-terminal-alacritty`
  repo, which made one engine unit the owner of the rules every engine unit is judged by.
  It does not any more.
- **`reference_states/`** — the declared screens. For each fixture: *this stream must produce this
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
is not a hypothetical: SPEC.md §13 records the defect it missed and the reference_state found.

The standard is the declared reference_state. Every engine, Alacritty included, is an equal
candidate graded against it.

## How a unit is graded

Three axes, each an ordinary assertion:

1. **Interpretation** — feed the corpus stream; the mirror's screen state equals the reference_state.
2. **Restore** — feed that mirror's `rehydrate` paint to a fresh mirror of the same engine;
   its screen state equals the *same* reference_state. The reference_state being external is what stops a
   self-consistent error from hiding.
3. **Replay guard** — no byte leaves the mirror, no query rides in the paint.

A unit's `tests/conformance.rs` implements `MirrorUnderTest` (the one thing it owes: turn
its engine's representation into the canonical form) and calls `assert_conforms` from seven
plain `#[test]` functions.

## Where a rule may come from

Every value the contract declares answers to one ladder (SPEC.md §11.A), and **an engine is
not on it at any rung**. The rungs are: xterm's `ctlseqs` and manual page (our domain's
specification — the core spawns shells with `TERM=xterm-256color`, and the manual's RESOURCES
section is the only place initial states are written down); then ECMA-48 and DEC VT510 for
what a sequence *means*; then UAX #11 for width; then terminfo as corroborating data. When all
of them are silent, the contract decides and records the decision in the silence table
(SPEC.md §11.S) with the argument that forced it.

**The contract also declares the state a mirror is born in** (SPEC.md §11.I). It has to: a
reference_state declares the whole screen, and a screen includes the modes a stream never mentioned.
Those values used to come from a running engine — and §13 records what that cost.

## Bootstrapping a reference_state

An engine's output can propose a candidate — `SOKSAK_REFERENCE_STATE_OUT=<dir> cargo test --test
conformance -- --ignored dump_reference_states`, run from a unit — and comparing the candidates of
several independent engines is a cheap way to find the places worth thinking about. But
agreement is evidence, not authority: four engines agreeing on a wrong answer produce a wrong
reference_state that the suite will then defend forever. A candidate becomes a reference_state only once it is
argued against the ladder, and that argument is written into the file — where a test enforces
it (`tests/reference_states_cite_specs.rs` fails if an engine's name appears in a reference_state's reasoning,
or if the reasoning cites nothing at all).

## Current standing

| unit | fixtures | performance floor |
| --- | --- | --- |
| `soksak-sidecar-terminal-vt100` | 7 / 7 (on the fork that adds DEC Special Graphics) | ok |
| `soksak-sidecar-terminal-alacritty` | 7 / 7 | ok |
| `soksak-sidecar-terminal-wezterm` | 7 / 7 (on the fork that makes a wide character obey DECAWM at the margin) | ok |
| `soksak-sidecar-terminal-ghostty` | 7 / 7 | ok (closest to the line) |

Both of the engines standing on a fork are there because this suite found a real defect in
them, and both defects were closed at their owner rather than papered over in the unit. The
suite also found two bugs in the *mirror's own code*, in every unit: a style left active across
a line break, which bleeds colour on any terminal that erases with the current background; and
a restore paint that turned alternate scroll **off** in the user's terminal for every session
that had never mentioned it — because the contract's idea of a fresh terminal had been read off
an engine. SPEC.md §13 has them all, with the reasoning.

All four clear the performance floor today — but one of them did not, and how that was settled is
the point. The wezterm mirror fed at 68 MB/s against a demand of ~85, and held at its own rate
against a real daemon it **lost 16.5 MB of a 67 MB flood**: with the app closed and a session
dumping output, a quarter of the screen it exists to restore never reached it. Under the old floor
(50 MB/s, read off the candidates) it had passed comfortably. The floor was not lowered; the mirror
was made faster (68 → 102 MB/s), and it now drops nothing. SPEC.md §14.3.

## The gate

The contract repository verifies only its own declared states, report schema, vocabulary, and
assertion implementation:

```sh
make verify
```

The exact Rust toolchain is owned by `rust-toolchain.toml`. This repository does not discover or
execute provider repositories. Each provider runs the shared contract cases against its own
implementation from its own `make verify TARGET=<native-target>` gate; the installed-system
repository owns the real multi-provider composition.

**A unit passes its owner boundary when its repository `make verify TARGET=<native-target>` passes.**
That command keeps the seven reference-state fixtures, unit tests, and owner performance budgets
blocking. The benchmark is `#[ignore]`d in the ordinary test run — it would
slow the development loop — so a budget that only ran when someone remembered to ask for it
would have been a comment, not a budget. The gate is what makes it binding.

The contract table compares owner reports only. The installed terminal system-test repository owns
real PTY demand, gap, and tail-marker evidence because it owns the multi-component composition.
Provider repositories do not build or execute the PTY implementation. The old relative guard — no
unit below a quarter of the fastest in the same run — remains deleted.

## Performance comes from demand, not from the candidates

The floor used to be 50 MB/s, sitting just under the slowest unit, with a second guard that
compared the candidates to each other. Both numbers were the candidates', not the contract's.

The budgets are now derived from the one requirement there is — *the mirror must not be the
reason a tee gap happens*. Turning that into a number takes one fact, and it is a fact about the
daemon, not the mirror: the daemon pauses reading the pty only while a front end is **attached**
and behind. With the app open, the front end paces the river and everything is easy (the core's
own gate measures 3.3–4.6 MB/s end to end). With the app **closed** — the mode the mirror exists
for — nothing paces it at all, and a mirror slower than the daemon's tee delivery simply loses
bytes.

The provider owner floor is a fixed **80 MB/s**, rounded up from the independently measured
reference PTY demand rather than read from provider candidates. Terminal system tests separately
measure the **real installed PTY** against each installed provider and require zero gap plus final
marker delivery. A composition gate must include daemon synchronization, frame queuing, rendering,
IPC, and acknowledgement. Estimates that omitted those costs overstated throughput by 2.4× and
about 25×, so only the installed composition path is acceptance evidence.

## No default unit

Every unit that clears the gate is an equal choice, and the plugin manifest must name the one
it wants. There is no default, because an implicit default is a ranking wearing a shrug: the
moment one exists, every other unit is a deviation from it. Supply-chain facts (a fork, a pinned
commit) are recorded because a plugin author needs them — they are not a grade.

## Licensing

The contract bundles no engine and depends on none, so it carries no engine's license.
Each unit carries its own.
