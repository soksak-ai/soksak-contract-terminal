# soksak-contract-terminal — the terminal sidecar contract

Contract id: **`soksak-sidecar-terminal-spec@1`**.

Normative wire between a terminal-domain sidecar and the terminal plugins that
consume it, and between that sidecar and the core PTY daemon (`soksak-ptyd`) it
peers with. The English text is canonical.

**The contract does not live inside an implementation.** It used to: this document
sat in the `soksak-sidecar-terminal-alacritty` repo, which made one engine unit the
owner of the rules every engine unit is judged by. It owns them no longer. The
contract, the corpus, the declared goldens, and the acceptance assertions live here,
in a repo that implements nothing and ships nothing.

**Repo name vs contract id.** The repo is `soksak-contract-terminal`; the contract id
is `soksak-sidecar-terminal-spec@1`. They differ on purpose. The id is a string value
that names *the contract a sidecar unit implements* — manifests and code keep using
it unchanged. The repo name says what this repo *is*: a contract, not a sidecar. It
has no binary, no `dist`, no registry entry, and nothing is installed on a user's
machine; engine units consume it as a **dev-dependency** and that is the only way it
is consumed.

**One contract, many engine units.** `soksak-sidecar-terminal-alacritty`,
`-wezterm`, `-vt100`, and `-ghostty` are separate units implementing this same
contract — one at a time behind a terminal plugin's declaration. The unit name
carries the engine, exactly as `soksak-sidecar-browser-chromium` carries Chromium.

## 1. Purpose and boundary

A terminal-domain sidecar mirrors and restores terminal screen state. It is the
terminal domain's owner of screen synthesis, ANSI serialization, and checkpoint
policy — the parts that read the *meaning* of terminal bytes. It is not the byte
carrier: PTY spawn, byte survival, the raw ring, flow control, and the sealed-blob
store stay in the core daemon (`soksak-ptyd`), which understands none of this.

The split is fixed by ownership, not by convenience:

- **Core daemon (plumbing, byte-agnostic):** owns every shell, the raw output
  ring with a monotonic sequence, `Attach{from_seq}`, the tee subscription face,
  and a content-agnostic sealed-blob store. It never interprets a byte.
- **This sidecar (terminal domain):** consumes the tee, feeds a VT mirror,
  serializes the grid to paint, and decides when and what to checkpoint. It never
  touches a shell, a pid, or a key.

## 2. Runtime shape

Service-model sidecar (headless, no surface) with a **survival requirement**: it
must outlive every process that spawned it, because it checkpoints shells that
themselves outlive the app. A stdio service dies with its spawner's pipe, so this
sidecar does not use stdio; it binds a rendezvous socket in the identity home and
peers with the daemon — the same transport shape as `soksak-ptyd` (SIDECARS.md
§1). Spawned detached (`process.detached`); a fresh start probes the socket and
exits when a live instance already answers (singleton). Death is loud at the
consumer, never silent.

## 3. Two socket faces

- **Server face** — terminal plugins connect and request restore. Messages in §5.
- **Consumer face** — the sidecar connects to the daemon as a client: it
  subscribes to the per-session tee (raw byte copy → mirror feed) and pushes
  serialized plaintext state to the daemon's sealed-blob store. §6.

## 4. Wire

NDJSON over a Unix domain socket in the identity home (`home.rs` derivation, one
socket per identity — never shared across identities, A17). One JSON object per
line, request then reply. The `hello` handshake is isomorphic to the daemon's
`hello` (a `{version, token}` object); a version mismatch is refused loudly, never
downgraded.

## 5. Server face — messages (plugin → sidecar)

**How the plugin reaches this socket.** A webview cannot open a Unix domain
socket, so the plugin does not connect directly; the core relays one NDJSON
request/response to this service socket on the plugin's behalf
(`pty.sidecarRequest` → `pty_sidecar_request`, the same layer as the core PTY
byte bridge). The relay is content-agnostic — it passes the request and reply
through untouched and only stamps the plugin's window routing coordinate, as
`spawn` already does. This is **the contract-consistent warm path** over the
alternative of routing warm through the daemon's sealed-blob store: warm needs
the *live* mirror's `rehydrate` serialization and a fresh `uptoSeq` computed
against the current alt-screen state at request time (§ below), which a
debounced checkpoint blob — storing the flattened `cold_paint`, stale by its
policy, and carrying no live sequence — cannot provide. The seal store is the
right home for **cold** (§7), where there is no live mirror; warm is a live
request-response, so it rides the service socket.

- `hello{version, token}` → `{ok}` — version handshake and identity-home token
  check. Mismatch refuses loudly.
- `ensureSession{window, pane, cols, rows}` → `{subscribed}` — subscribe this
  pane's live daemon session if not already mirrored, and set the mirror grid.
  A tee delivers only bytes emitted after the subscription and the daemon does
  not announce new sessions, so a session born after the sidecar started is not
  auto-subscribed. A terminal plugin calls this right after it spawns a terminal
  so the sidecar catches that session's tee near birth; it is idempotent
  (already-mirrored panes only refresh the grid). The sidecar resolves the
  `(window, pane)` to a live daemon session (`listSessions`) and subscribes,
  anchoring `consumedSeq` to the subscribe ack's `startSeq` (§6.2). No live
  daemon session for the pane is a loud `NOT_FOUND`.
- `rehydrate{window, pane}` → `{paint, uptoSeq, altActive}` — **warm**. The
  session is live in the daemon and the mirror is fed from the tee. `paint` is the
  serialized grid reflecting raw-ring output through sequence `uptoSeq`;
  `altActive` reports whether the alt-screen is active. The consumer paints, then
  attaches the daemon raw stream from `uptoSeq` (`Attach{from_seq: uptoSeq}`). The
  sequence boundary is what makes the handoff race-free: the synthesized paint
  carries no query (the mirror never answers), so no DA1/DSR is replayed twice;
  queries in the raw tail after `uptoSeq` are genuine unanswered queries the live
  terminal answers once.
- `coldPaint{window, pane}` → `{paint, altActive}` — **cold**. The session is not
  live. `paint` is the flattened inert screen (an active alt-screen is flattened
  into the text flow — a dead session's TUI is a snapshot, not a live screen).
  No sequence handoff — there is no live stream to attach.
- `resize{window, pane, cols, rows}` → `{ok}` — the tee carries output bytes
  only, not the terminal size (resize is a control op, not a byte in the stream).
  A consuming plugin knows the pane geometry and pushes it so the mirror grid
  matches; until told, the mirror defaults to 80×24. A wrong grid width mis-wraps
  the restored paint, so this closes that gap.
- `status` → `{sessions, checkpointAges, suppressedReplies, teeGaps}` —
  introspection over the socket. `teeGaps` counts backpressure gaps the sidecar
  received from the daemon tee (a dropped-byte discontinuity is never silent).
  No side effect.

## 6. Consumer face — daemon peering (sidecar → daemon)

The sidecar is a client of the daemon's two sockets under the identity home —
the same paths the app uses (`ptyd-p<N>.sock`, `ptyd-p<N>-stream.sock`,
`ptyd-p<N>.token`, all protocol-keyed by `PTYD_PROTOCOL_VERSION`). This repo does
not link the daemon's crate; it implements the documented wire below, exactly as
the browser sidecar implements a documented engine ABI rather than linking core.

### 6.1 Control socket (NDJSON request/response)

Connect, send the `hello` line, read the `ok` line, then issue tagged requests
(`{op: …}`). The requests the sidecar issues:

- `listSessions` → the live session set (`session`, `paneId`, `windowLabel`,
  `shellPid`, `generation`) — which tees to subscribe to.
- `getSnapshot{session}` → `{snapshotB64}` — a one-shot mirror-replay used only
  to seed a mid-session start (§6.4).
- `storeBlob{windowLabel, paneId, bytesB64}` → `{stored}` — sealed-blob push
  (§6.3).

### 6.2 Tee subscribe (stream socket, length-prefixed frames)

Open the stream socket, send `hello{version, token, clientId, session,
subscribe: true}`, read the one NDJSON ack line (`{ok, data:{session, mode:
"subscribe", startSeq}}`), then read **length-prefixed frames** until EOF. The
attach stream (a single live consumer) stays raw; a tee interleaves data copies
with gap markers, so a tee is framed. `startSeq` is the daemon ring head at the
moment the subscription was registered (read under the session lock, so exactly
the bytes after it reach this subscriber). The consumer sets `consumedSeq =
startSeq` and advances it by each data frame's length; this anchors the
warm-handoff coordinate to the ring even for a mid-session subscription, so a
later `rehydrate.uptoSeq` names a true ring sequence rather than a false zero.
Each frame:

```
[kind: u8] [len: u32 big-endian] [payload: len bytes]
```

- `kind = 0` (`TEE_FRAME_DATA`) — `payload` is a raw output copy; feed it to the
  mirror verbatim.
- `kind = 1` (`TEE_FRAME_GAP`) — `payload` is JSON `{"fromSeq":N,"toSeq":M}`: the
  half-open range `[fromSeq, toSeq)` the daemon dropped for this subscriber under
  backpressure. The mirror surfaces the discontinuity (a `teeGaps` counter,
  reported by `status`) — a slow subscriber loses data loudly, never silently.
  The daemon never blocks the live path to serve a slow tee.

### 6.3 Sealed-blob push

On the checkpoint policy (§ below), push the serialized plaintext state as
`storeBlob{windowLabel, paneId, bytesB64}` (base64 of `Mirror::cold_paint`). The
daemon seals it (X25519, `soksak-seal`) and writes it atomically to the
checkpoint path. **This sidecar never holds a key** — it hands plaintext to the
daemon and the daemon owns the crypto (single truth in core). `storeBlob`
requires a live session that was created with a checkpoint recipient key; a
keyless session fails closed (the daemon never writes plaintext screen bytes).

### 6.4 Seeding a mid-session start

A tee delivers only output produced after the subscription — output before it is
not in the tee. The design decision:

- **Near-birth subscription (the normal path).** A terminal plugin calls
  `ensureSession` right after it spawns a terminal, so the sidecar subscribes to
  that session's tee within the spawn→ensure window. `consumedSeq` anchors to the
  ack's `startSeq` (§6.2), so the coordinate is exact regardless of when it
  joined. Only the pre-subscription prefix (bounded to that tiny window — at most
  the shell's initial prompt) is absent from the mirror; nothing is
  silently mis-sequenced, and any ring eviction after subscription still arrives
  as a loud gap.
- **Deeper seeding (optional, respawn).** To also recover the pre-subscription
  prefix, a fresh mirror can be seeded from a one-shot `getSnapshot` before it
  feeds tee frames; the overlap/miss window is bounded and loud. When the
  daemon's own mirror retires (the core eviction), the daemon must keep an
  equivalent seed path (raw-ring replay from a sequence); the contract's seed
  obligation is "loud, bounded, never a silent shift", not a specific daemon op.
  The near-birth anchor above is the M3 path; the snapshot seed is the deeper
  fallback.

## 7. Failure semantics

- Sidecar death does not touch shells or the live path — the daemon owns byte
  survival. Only restore fidelity degrades.
- On a dead sidecar the consumer announces the degradation loudly, falls to the
  seal path (the plugin fetches the sealed blob from the daemon and opens it with
  the app vault — a path that needs no sidecar), and respawns the sidecar.
- Degraded restore is loud, never silent.

## 8. Engines are candidates, not authorities

No engine is canonical. The VT state machine that produces the paint is chosen per
unit, and every engine — including Alacritty — is an equal candidate graded by the
declared goldens (§11, §12). This matters because the alternative was tried: for a
while the acceptance suite rendered each unit's restore paint with the Alacritty
engine and compared it against Alacritty's own rendering of the raw stream. That made
"correct" mean "what Alacritty does", made the Alacritty unit its own judge, and — as
§13 records — could not see a real misinterpretation in another engine, because the
error was masked by the re-rendering. The standard is now declared data, and no
implementation sits above another.

**Licensing is per-unit.** Each engine unit carries the license and attribution of the
engine it bundles. The contract imposes none, and no license crosses between units.
This repo bundles no engine at all — it does not even depend on one.

## 9. Acceptance

A unit conforms when its mirror, graded against the **declared goldens** (§12) over
the **corpus** (seven fixtures: a ring cut mid-escape, a ring cut mid-UTF-8 with wide
characters, alt-screen with a frozen primary, private modes beyond the ring window,
the replay guard, cold paint of an alt-screen TUI, and DEC line drawing), satisfies
all three axes:

1. **Interpretation.** Feed the corpus stream; the mirror's screen state (§11) equals
   the golden.
2. **Restore.** Feed that mirror's `rehydrate` paint to a **fresh mirror of the same
   engine**; its screen state equals the **same golden**. Because the golden is
   external, an engine that misreads the stream and then re-misreads its own paint the
   same way does not pass — a self-consistent error has nowhere to hide.
3. **Replay guard.** No byte leaves the mirror, the paint carries no query bytes, and
   swallowed queries are observable.

The suite is plain assertions called from ordinary `#[test]` functions; a unit stands
its mirror up through `MirrorUnderTest` and calls `assert_conforms`. There is no
runner, no harness, and no copy of the fixtures in any unit.

## 10. Who may consume

Any terminal plugin. Input is a raw byte stream and output is ANSI paint — no
consumer couples to a specific engine. M3 wires both `soksak-plugin-terminal`
(xterm) and `soksak-plugin-terminal-ghostty`; each declares it in the manifest:

```json
"sidecars": [
  { "name": "terminal-alacritty",
    "interface": "soksak-sidecar-terminal-spec@1" }
]
```

**The plugin manifest selects the unit.** `interface` pins the contract
(`soksak-sidecar-terminal-spec@1`); `name` picks which engine unit implements it
(`terminal-alacritty` today, `terminal-wezterm` when a plugin ships it). One
contract, one running engine unit behind a plugin at a time — declaring it is the
whole cost of consuming it.

## 11. Screen state — the canonical form

Grading a screen requires saying what a screen *is*, and what makes two screens the
same. Engines represent the same picture differently, and until now those differences
were settled implicitly by whichever engine happened to be the judge. They are settled
here instead, by rule. The types are in `src/state.rs`; the rules are these.

**N1 — Colour is an index or an RGB triple, never a name.** A palette colour is its
index. Some engines split the palette into "named" (0–15) and "indexed" (16–255); that
split is one engine's internal representation, not a property of the screen. Default
foreground and background stay *default* — they are theme-relative and are never
resolved to concrete RGB.

**N2 — A trailing run of blank cells is not part of the row.** Whether a terminal
padded the row to its full width with spaces or simply never touched those columns, the
screen is the same, so the canonical row ends at its last non-blank cell.

**N3 — A wide character is one cell that occupies two columns.** The body cell carries
the character and is marked wide; the column it also covers is not represented. Engines
that store an explicit spacer cell and engines that do not therefore compare equal.

**N4 — A cell with no text is a space.** An "empty" cell, however an engine encodes it
(codepoint zero, an empty string, a sentinel), is a space.

**N5 — Underline is a boolean.** The contract restores SGR 4; a double, curly, dotted or
dashed underline is not distinguished, because the contract does not promise to carry
the distinction.

**N6 — A space carries only the attributes that are visible on a space.** A space has no
glyph, so a foreground colour, bold, dim, italic, and hidden draw nothing on it; only the
background, inverse, underline, and strikeout do. The canonical form therefore drops the
invisible ones from a space. (Inverse is the exception that proves it: under inverse the
foreground *becomes* the visible background, so it is kept.)

  N6 is not a convenience. It was forced by an observed engine difference: at least one
  engine, when the cursor auto-wraps to a new line, leaves the untouched cells of that
  line carrying the pen's current SGR. Those cells look like blanks on screen and differ
  only in the model. Folding them is the contract choosing the screen over the model —
  and it is the rule that keeps a representation difference from being reported as a
  defect.

**N7 — Cursor visibility is `show_cursor`, once.** DECTCEM is a mode; the canonical form
does not also carry a separate cursor-visible flag.

## 12. Goldens — the declared screens

For each fixture the contract declares the screen the stream must produce: `goldens/`,
one text file per fixture, each opening with the reasoning that puts it there. The
format is data, not a language — one line per value, one line per row, each row preceded
by its plain text so the file can be read as a screen and reviewed as a table.

A golden is **declared, not recorded**. An engine's output may be used to bootstrap a
candidate (`dump`, behind `--ignored`), and cross-checking the candidates of several
independent engines is a cheap way to find the places worth thinking about — but
agreement is evidence, not authority. What makes a golden a golden is the reasoning
against the terminal specification written at the top of the file: fixture ⑦ declares
box glyphs because the DEC Special Graphics table maps `l` to U+250C, not because some
engine drew a box.

Where the engines disagreed, the contract judged. §13 records those judgements.

## 13. Candidate review

**Wide character at the right margin — three engines against one.** With 79 columns
filled and one column left, the corpus prints a double-width character. A width-2
character occupies two adjacent columns (UAX #11) and cannot be placed in one; with
autowrap on (DECAWM, the default), a character that does not fit before the right margin
wraps. So the row ends at 79 columns — the last column reserved for the character that
moved — and the character begins the next row. Alacritty, vt100, and ghostty all do this,
and two of them keep a dedicated cell state for the reserved column (Alacritty's
`LEADING_WIDE_CHAR_SPACER`, ghostty's `WIDE_SPACER_HEAD`), which is independent testimony
that the wrap is the rule. **wezterm-term instead packed the character into the last
column**, yielding a row that claimed 81 columns of content in an 80-column grid and a
scrollback one row short: its print path checked only whether the cursor had passed the
margin, never whether the grapheme fit in what was left. The golden declares the wrap.

  Closed at its owner, as the vt100 charset gap was. A local fork adds the missing check —
  under DECAWM a grapheme wider than the remaining columns moves to the next line — and
  against that engine the unchanged suite is 7 of 7. Release eligibility waits on the fix
  reaching a published crate; a patch for wezterm upstream is prepared.

  This is the finding that condemns the previous acceptance design. Under the old suite —
  which rendered every unit's restore paint with the Alacritty engine and compared it to
  Alacritty's rendering of the raw stream — wezterm passed all seven. It passed because
  its serializer emits text, and Alacritty, replaying that text, wrapped the wide
  character correctly; the misinterpretation inside wezterm's own grid was erased by the
  re-rendering. Only a declared golden, compared against the engine's own screen, can see
  it.

**Pen-coloured blanks after a line break — one representation difference, one real bug.**
wezterm-term fills the untouched cells of a newly exposed line with the pen's current SGR
(background-colour-erase, which the other three engines do not do on this path). Two
different things came out of that.

  *The representation difference.* When the pen carries only a foreground colour and bold,
  those cells are blanks on screen and differ only in the model. N6 folds them. A standard
  that cannot tell a difference in the model from a difference on the screen manufactures
  false defects, and this is the rule that stops it.

  *The real bug — ours.* When the pen carries **inverse**, the fill is not invisible: an
  inverted blank shows the foreground as a solid block. Chasing it found a defect in the
  mirror's own serializer, in every engine unit: the restore paint emitted `\r\n` with a
  style still active, so a terminal that erases with the current background bled that style
  across the rest of the newly exposed line. The raw stream never does this — it resets SGR
  before the newline — and the paint must not either. The serializer now resets before every
  line break.

  This one is worth dwelling on, because it is not an engine's bug: it is a restore-fidelity
  bug that would bleed colour into a real user's screen, on any terminal that implements
  background-colour erase — which the front-end terminals do. The old acceptance could not
  see it. It rendered the paint with Alacritty, and Alacritty does not fill on this path, so
  the bleed had nothing to land on. It took an engine that does fill, graded against a
  declared golden, to make it visible.

**vt100 — an engine capability gap, closed at its owner.** The published `vt100` 0.16.2
does not implement DEC Special Graphics: it ignores `ESC ( 0` and treats SI/SO as no-ops,
so a line-drawing border is mirrored as literal ASCII. That was 6 of 7. A local fork adds
the designation, the SO/SI invocation, glyph translation on print, DECSC/DECRC of the
charset state, and persistence across the alternate screen; against that engine the
unchanged suite is 7 of 7. Release eligibility waits on the support reaching a published
crate.

**ghostty — a seat misconfiguration, not an engine gap.** The engine's scrollback limit
is a byte budget, not a line count (the C header's wording notwithstanding), and pruning
drops the oldest whole page. A budget sized to the restore window collapses below the
window the moment pruning fires — the mirror kept 588 rows where the corpus demanded
1000. The engine was right; the unit's configuration of it was not.

**avt, shpool_vt100 — rejected on the record.** Neither maintains the scrollback and
private-mode state the contract restores, so neither reaches the fixtures.
