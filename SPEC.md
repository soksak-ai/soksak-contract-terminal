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

**Deleting the judge engine was not enough**, and it is worth being precise about why.
An engine can set the standard without ever being called as a judge: it can simply be
the place a value was read from. That is what happened to the state a mirror is born in
(§11.I) and to the performance floor (§14) — no engine judged anything, and both numbers
were still an engine's. **§11.A is the rule that closes the second door**: it says where
a value may come from, and an engine is not on the list at any rung. Everything below —
the canonical form, the goldens, the budgets — answers to it.

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

**The plugin manifest selects the unit, and there is no default.** `interface` pins the
contract (`soksak-sidecar-terminal-spec@1`); `name` picks which conforming engine unit
implements it. A plugin that names no unit is an error, not a plugin that gets one chosen
for it — an implicit default is a ranking wearing a shrug, and the moment one exists every
other unit is a deviation from it. Every unit that clears the gate is an equal choice; the
plugin author makes the choice and writes it down.

**Supply-chain facts are recorded, not graded.** Two engines currently run on a local fork
that closes a defect this suite found, and one runs on a pinned commit of a library its own
authors call unstable (§13). Those are true, they matter to whoever picks a unit, and they
belong in the record — but they are not a rung on any ladder here. This contract judges one
thing: does the unit produce the declared screen, and does it clear the floor. A unit that
does both is conformant, and nothing about its packaging makes it more so.

## 11. Screen state — the canonical form

Grading a screen requires saying what a screen *is*, and what makes two screens the
same. Engines represent the same picture differently, and until now those differences
were settled implicitly by whichever engine happened to be the judge. They are settled
here instead, by rule. The types are in `src/state.rs`; the rules are these.

### 11.A The authority ladder — where a rule may come from

A standard is only as good as its sources. Every value this contract declares — every
golden cell, every mode, every initial state — is answerable to this ladder, and to
nothing else. **An engine is not an authority at any rung.** Four engines agreeing is
evidence that a question is settled somewhere; it is never the settlement.

**Rung 1 — xterm: `ctlseqs` and the manual page.** Not because xterm is an
implementation we like, but because it is the *specification of our domain*. The core
spawns every shell with `TERM=xterm-256color` (`src-tauri/src/pty.rs`), so the shell
emits what an xterm-compatible terminal is documented to accept, and the front end that
renders it is xterm.js or a ghostty webview. The documents are therefore not one
vendor's behaviour — they are the written interface both ends of our system were built
against. `ctlseqs` gives the sequences; the manual's **RESOURCES** section gives the
**initial states** (`autoWrap`, `appcursorDefault`, `alternateScroll`, …), which no
other document in the ladder supplies.

**Rung 2 — ECMA-48 and DEC VT510.** For the *meaning* of a sequence, and for structure
(what a control function is, what a mode is). **Not for initial values.** A DEC
power-on default is a fact about a 1980s glass terminal, not a norm for a mirror inside
a 2020s workspace; where rung 1 states an initial state, rung 2 does not get a vote,
and where rung 1 is silent, we go to rung 4 rather than borrow DEC's power-on habits.
(Rung 2 may still *corroborate* — noted as corroboration, never as the ground.)

**Rung 3 — UAX #11.** Character width. Nothing else in the ladder defines it.

**Rung 3.5 — terminfo (data, not implementation).** The terminfo database is the
capability contract the *shell* writes against: it is data every program consults, not
one program's behaviour. `terminfo(5)` defines `am` as "terminal has automatic
margins", and the ncurses source (`terminfo.src`, entry `xterm-basic`, which
`xterm-256color` inherits through `xterm-new` → `xterm-p370`) declares `am` — as does
the installed database (`infocmp xterm-256color`). It corroborates rung 1; it does not
overrule it.

**Rung 4 — we decide, and we write down why.** When every document above is silent, the
contract makes the call from first principles (grid geometry, the render model, what a
program can and cannot rely on) and records it in the **silence table** (§11.S). A
decision on this rung is still a decision *of the contract* — it is never "whatever the
engines happened to do".

### 11.I The initial state — a mirror is born here

A golden declares the whole screen, and a screen includes the modes that were never
mentioned in the stream. Those values have to come from somewhere, and until now they
came from whichever engine was consulted: Alacritty's `TermMode` default carried
`ALTERNATE_SCROLL`, and the other three units were built to agree with it — two of them
by writing `alternate_scroll: true` into their own seat. That is a standard set by a
candidate, in the purest form.

**The contract declares the state a mirror is born in.** An engine whose power-on
default differs is put into this state at birth by its unit; the unit's seat owes the
contract that, exactly as it owes it the canonical form. The engine's default is not a
fact about the screen — it is a fact about the engine.

| born state | value | where it comes from |
| --- | --- | --- |
| `line_wrap` (DECAWM) | **set** | Rung 1, xterm manual, RESOURCES: *"autoWrap (class AutoWrap) — Specifies whether or not auto-wraparound should be enabled. This is the same as the VT102 DECAWM. **The default is "true"**."* Corroborated at rung 3.5: the `xterm-256color` entry declares `am`. |
| `show_cursor` (DECTCEM) | **set** | Rung 1, xterm manual, *Do Soft Reset* — of the states both soft and full reset establish: *"**Make the cursor visible**, with shape reset according to the cursorUnderLine and cursorBar resources."* |
| `app_cursor` (DECCKM) | **reset** | Rung 1, xterm manual: reset *"resets DECCKM and DECKPAM per resources appcursorDefault and appkeypadDefault"*; *"appcursorDefault … **The default is "false"**."* |
| `app_keypad` (DECKPAM / DECNKM 66) | **reset** | Rung 1, same clause; *"appkeypadDefault … **The default is "false"**."* |
| `alternate_scroll` (1007) | **reset** | Rung 1, xterm `ctlseqs`: *"The initial state of Alternate Scroll mode is set using the **alternateScroll resource**."* → xterm manual: *"alternateScroll (class ScrollCond) … **The default is "false"**."* |
| `bracketed_paste` (2004), `mouse_click` (1000), `mouse_drag` (1002), `mouse_motion` (1003), `sgr_mouse` (1006), `utf8_mouse` (1005), `focus_in_out` (1004) | **reset** | §11.S **S4** — the documents name no initial value, so the contract decides. |
| `insert` (IRM) | **reset** | §11.S **S5**. |

The mode vector of any golden whose stream never touches a mode is therefore a
restatement of this table; fixture ⑤ (`replay_guard`) is the one that says nothing else,
so it is where the birth state is pinned.

**The rule has teeth beyond the goldens.** The restore paint may only mention a mode the
*session* changed. While the contract's initial state was Alacritty's, every unit's paint
emitted `ESC[?1007l` for a session that had never heard of mode 1007 — quietly turning
alternate scroll **off** in the user's terminal on every restore, because one engine's
power-on default said it was on and the paint was written to reconcile against that.
Deriving the birth state from the specification deleted that line.

### 11.S The silence table — what we decided, and why

When the ladder is silent, the contract decides. Each decision is listed here with the
silence that forced it, so that a future reader can tell a *judgement* from a
*specification* at a glance — and can overturn a judgement without touching a
specification.

| # | question | who is silent, and why | our decision | the argument |
| --- | --- | --- | --- | --- |
| **S1** | A double-width character with **one column left** before the right margin. | VT510's DECAWM clause speaks of the cursor being *at* the right border, not of a character that *does not fit* in what is left; DEC terminals had no double-width characters, so the case did not exist for them. UAX #11 defines width and says nothing about terminal wrapping. `ctlseqs` does not treat the case either. | The character **moves to the next line**; the reserved column stays blank. | A width-2 character occupies two adjacent columns (UAX #11). Placing it in column 79 of an 80-column grid needs column 81, which does not exist. The remaining options are (a) wrap, (b) drop the character, (c) draw half of it. (b) loses input bytes; (c) is not expressible in a cell model. So (a). |
| **S2** | Which Unicode codepoints the **DEC Special Graphics** glyphs are. | DEC named glyphs (VT100 User Guide, Table 3-9) in an era with no Unicode; `ctlseqs` defines the *designation* (`ESC ( 0`) but ships no glyph table. | The Box Drawing characters whose **Unicode names describe the same geometry** as DEC's glyph names. | The mapping is forced, not chosen: DEC's "upper-left corner" and U+250C BOX DRAWINGS LIGHT DOWN AND RIGHT describe one shape. Light weight, because DEC's set draws a single-weight grid — Heavy or Double would invent a distinction DEC did not make. |
| **S3** | The initial state of **mouse reporting** (1000/1002/1003/1005/1006), **bracketed paste** (2004) and **focus reporting** (1004). | `ctlseqs` defines each as *Enable X* / *Disable X* and gives no initial value; the manual has no resource for any of them. | **Reset** (off). | These modes change what the terminal *sends to the application*. A terminal that reported mouse clicks, or wrapped pastes in brackets, without being asked would corrupt the input of every program that never asked — and every program that wants them enables them first. A ground state that breaks the programs older than the feature is not a ground state. |
| **S4** | The initial state of **IRM** (insert vs replace). | Rung 1 does not list IRM among the states a reset establishes. | **Replace.** | Insert mode changes what *every* subsequent printed character does to the row it lands on. A terminal born in INSERT would shove the existing line rightwards on every write, so no program that assumes overwrite — which is all of them until they say otherwise — could print correctly. *(Corroboration, not ground: ECMA-48 Table 6 lists REPLACE as IRM's reset state, and §7.1 recommends that "the reset state of the modes be the initial state".)* |
| **S5** | What a **space** shows. | No specification says which cell attributes are visible on a blank glyph — that is a question about rendering, not about the byte stream. | Only `bg`, `inverse`, `underline` and `strikeout` survive on a space; `fg`, `bold`, `dim`, `italic` and `hidden` are dropped. | See **N6**: it is a statement about the render model, and it is argued there. |
| **S6** | How big the sidecar may be. | Nothing outside this contract says how much memory a background mirror service may hold. | **16 panes inside 512 MB → 32 MB per mirror** (§14.2). | A workspace's panes are all mirrored at once, and the sidecar is a *background* service: it may not compete with the app for memory. The promise is stated in the currency a user has (panes), not in the currency an engine has (cells). |
| **S7** | Everything the **performance budgets** rest on. | No specification says how fast a terminal mirror must be. | Derived from the system's measured demand, never from what the candidates happen to achieve. | See **§14**. |

Every rule below is answerable to §11.A. Where a specification decides the question, it
is cited; where none does, the rule is a contract decision and says so, with its entry in
the silence table (§11.S).

**N1 — Colour is an index or an RGB triple, never a name.** SGR names no colours: it
numbers them. ECMA-48 §8.3.117 (SGR) gives 30–37 / 40–47 as numbered parameters, and
`ctlseqs` extends the same numbering to 256 indices and to a 24-bit triple (`CSI 38 ; 2 ;
R ; G ; B m`). "Named" (0–15) versus "indexed" (16–255) is a split some engines make
internally; no document makes it, and the screen does not have it. Default foreground and
background stay *default* — a terminal resolves them against its theme at paint time, so
resolving them here would freeze one theme into the standard.

**N2 — A trailing run of blank cells is not part of the row.** Contract decision (no
document speaks of row equality). The argument is the render model: a cell the terminal
padded with a space and a cell it never touched paint the same pixels, so no observer of
the screen can tell them apart. Comparing them would grade the engine's memory, not the
screen.

**N3 — A wide character is one cell that occupies two columns.** UAX #11 §5: a Wide
character, "in fixed-pitch fonts … take[s] up one Em of space", and one Em on a
character grid is two cells. So the character *is* one thing standing on two columns.
Whether an engine also stores a spacer object in the second column is bookkeeping; the
screen shows one glyph over two columns either way, so the canonical form carries the
body cell and marks it wide.

**N4 — A cell with no text is a space.** Contract decision. A grid has no "absence" to
render: every column of every row paints something, and what an untouched column paints
is a blank. Engines encode that blank as codepoint zero, an empty string, or a sentinel;
those are three spellings of one screen.

**N5 — Underline is a boolean.** The contract restores SGR 4 (ECMA-48 §8.3.117, "singly
underlined"). The styled underlines (`CSI 4 : 3 m` and kin, `ctlseqs`) are not carried,
and the canonical form does not pretend to distinguish what the paint does not preserve.
A standard that graded a distinction its own restore path drops would be grading a lie.

**N6 — A space carries only the attributes that are visible on a space.** Contract
decision (§11.S **S5**), and it is a statement about **rendering**:

  A space has no ink. Painting a cell means drawing a glyph in the foreground colour on a
  field of the background colour — and where there is no glyph, everything that acts *on
  the glyph* draws nothing. Foreground colour, bold, dim, italic and hidden all describe
  how the glyph is drawn, so on a space they describe the drawing of nothing. What remains
  visible on an empty cell is exactly what does not need a glyph: the ones that **fill or
  invert the field** (background, inverse) and the ones that **draw a line through the
  cell** (underline, strikeout). Those are kept; the rest are dropped.

  Inverse is the case that proves the rule rather than breaking it. Under inverse the
  foreground and background swap, so the foreground colour stops describing a glyph and
  starts painting the field — it becomes visible precisely because it is no longer being
  used as ink. So a space under inverse keeps its foreground, and the rule is unchanged:
  *keep what is visible on a blank cell.*

  This is what makes the difference between a difference in the model and a difference on
  the screen. An engine that leaves the pen's colour on the cells of a line it has just
  exposed, and an engine that leaves them clean, have produced the same screen; a standard
  that could not say so would manufacture a defect out of bookkeeping. It is also why the
  rule is *narrow*: it folds only what cannot be seen. When the pen carries **inverse**,
  the fill is not invisible — and §13 records the real restore bug that hid behind exactly
  that case.

**N7 — Cursor visibility is `show_cursor`, once.** DECTCEM is a mode (`ctlseqs`: `CSI ? 25
h` / `l`, "Show cursor (DECTCEM), VT220"), and the canonical form already carries the mode
set. A second, separate flag for the same fact could only ever be a way for the two to
disagree.

## 12. Goldens — the declared screens

For each fixture the contract declares the screen the stream must produce: `goldens/`,
one text file per fixture, each opening with the reasoning that puts it there. The
format is data, not a language — one line per value, one line per row, each row preceded
by its plain text so the file can be read as a screen and reviewed as a table.

A golden is **declared, not recorded**. An engine's output may be used to bootstrap a
candidate (`dump`, behind `--ignored`), and cross-checking the candidates of several
independent engines is a cheap way to find the places worth thinking about — but
agreement is evidence, not authority. Four engines agreeing on a wrong answer produces a
wrong golden and a suite that will never see it again.

What makes a golden a golden is the **argument at the top of the file**, and that argument
answers to the ladder (§11.A): it cites the specification that settles the question, or —
where none does — it names the silence-table entry where the contract decided (§11.S).
Fixture ⑦ declares box glyphs because Unicode's Box Drawing names describe the same
geometry DEC's glyph table names, not because an engine drew a box.

**The rule is enforced, not merely stated.** `tests/goldens_cite_specs.rs` fails the build if
an engine's name appears anywhere in a golden's reasoning, and fails it if a golden's
reasoning cites neither a specification nor a silence-table entry. Prose discipline decays;
a test does not.

Where the engines disagreed — and where they agreed on something the specification does not
say — the contract judged. §13 records those judgements.

## 13. Candidate review

**The initial state was an engine's, and all four units agreed with it.** This is the finding
that matters most, because nothing failed. Every golden declares the whole mode vector,
including the modes the stream never mentions — and those values had been read off a running
engine. Alacritty's `TermMode` default carries `ALTERNATE_SCROLL`, so `alternate_scroll = 1`
went into the goldens as the state a mirror is born in. The other three units were then built
to match: the ghostty seat read its engine's mode 1007 (also on by default), and the wezterm
and vt100 seats wrote `alternate_scroll: true` into their own initializers by hand. Four
units, unanimous, 7 of 7 — and the value was never anything but one engine's habit.

The specification says otherwise, and says it twice. `ctlseqs`: *"The initial state of
Alternate Scroll mode is set using the alternateScroll resource."* The xterm manual:
*"alternateScroll (class ScrollCond) … The default is "false"."* The contract now declares the
birth state from those documents (§11.I), and the units are put into it — the standard moved
the engines, which is the only direction that was ever allowed.

  **The bug it was hiding.** The restore paint is written as a delta from the state a fresh
  terminal is in. While that state was Alacritty's, every unit's `mode_sets` emitted
  `ESC[?1007l` whenever the mirror's `alternate_scroll` was false — which is to say, for every
  ordinary session, none of which has ever heard of mode 1007. So a warm restore reached into
  the user's terminal and **turned alternate scroll off**, on a session that never asked for it
  to be either on or off, because one engine's power-on default said it was on and the paint
  existed to reconcile against that default. With the birth state derived from the
  specification, the line is simply gone: the paint now mentions 1007 only when the session
  set it. Nothing detected this. Nothing could: the suite compared the mirror to a golden that
  agreed with the engine, and the engine agreed with itself.

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

## 14. Performance — derived from demand, never from the candidates

The budgets used to say a terminal's real output "arrives at a few megabytes per second at
its very loudest", and set the floor at 50 MB/s. Both numbers were unsourced. Worse, the
floor sat just under the slowest unit measured, and the second gate — *no unit below a
quarter of the fastest in the same run* — compared the candidates against each other. A
standard whose numbers are read off the candidates is a standard the candidates set: let all
four regress together and the floor follows them down, and there is nothing left to notice.

So the budgets are re-derived from the **requirement**. There is exactly one:

> **The mirror must not be the reason a tee gap happens.**

A gap is a real loss: the daemon drops a slow subscriber's bytes rather than block the live
path (§6.2). It is loud, never silent — but a mirror that gaps is a mirror that lost the
screen it exists to keep. And no queue depth saves a *sustained* deficit: if the mirror is
slower than what feeds it, a long enough flood always overflows.

The requirement is one sentence. Turning it into a number takes a fact, and the fact is not
about the mirror at all — it is about **what paces the thing that feeds the mirror**. §14.1
measures it, in both of the modes the system actually runs in.

### 14.1 What actually feeds the mirror — measured, in both modes

The requirement resolves to a number only once you know **what paces the daemon's read loop**.
The daemon reads the pty, appends to the ring, copies into each tee subscriber's buffer, and
writes to the attached front end. Of those, exactly one can slow it down, and the source says
so in one line (`soksak-ptyd`, the output reader):

```
while st.paused && st.attached.is_some() { st = session.cv.wait(st).unwrap(); }
```

It pauses only while a front end is **attached** and behind by `HIGH_WATERMARK`. The tee is
never a brake — a subscriber that falls behind loses bytes as a recorded gap (§6.2). So the
answer differs by mode, and the contract has to look at both.

**Attached — the app is open.** The front end's acks pace the river. The core measures this
itself, end to end with rendering, in its own performance gate (`scripts/perf/budgets.json`,
scenario `t1_plain`): **3.3 – 4.6 MB/s**. Every unit clears that by more than an order of
magnitude. This mode does not constrain anything.

**Detached — the app is closed and the shells keep running.** *This is the mode the mirror
exists for.* And here nothing paces the daemon at all: no attach, no pause, no flow control.
The producer runs as fast as the daemon can drain the pty, and a mirror that cannot keep up
loses bytes. So the demand in this mode is simply **the rate at which the daemon delivers to
the tee**, and it must be measured against the real daemon — not modelled.

`src/daemon_demand.rs` does exactly that: it starts a real `soksak-ptyd`, spawns a session that
floods 64 MB through a pty, subscribes to the tee over the documented wire (§6.1, §6.2), and
reports the sustained arrival rate, the bytes the daemon dropped, and whether the marker printed
*after* the flood ever arrived. On the reference machine:

| subscriber | arrival | dropped (gap) | tail marker |
| --- | --- | --- | --- |
| as fast as it can | **77 – 90 MB/s** | 0 | arrived |
| held at 70 MB/s | 70 MB/s | **4.6 MB lost** | arrived |
| held at 95 MB/s | 80 MB/s | 0 | arrived |
| held at 154 MB/s | 78 MB/s | 0 | arrived |

That is the whole derivation. The demand is ~80 MB/s; a consumer below it **demonstrably**
loses data, one above it does not. Nothing here is inferred.

**Two models were tried before this measurement, and both were wrong in the flattering
direction.** A hand-built model of the tee pipe — the same syscalls, the same framing, the same
ring — reported 190 MB/s: **2.4× too fast**, because it did not pay the daemon's mutex, its frame
queue, its notify, or its separate writer thread. And the front end was modelled as xterm.js's
*parser* over the same corpus, 110 MB/s — about **25×** the rate the real front end acks at once
rendering and IPC are in the path (the core's own gate says 3.3–4.6). Both models have been
deleted rather than kept as context: a plausible model of a thing you can measure is a standing
invitation to skip the measurement. The gate measures the thing itself.

Measurement is regulated: release build, an otherwise idle machine, the median of three runs.
**Recalibration means re-running the measurement**, never lowering a number until the units fit
under it.

### 14.2 Budgets

| axis | budget | where the number comes from |
| --- | --- | --- |
| feed throughput | **≥ the daemon's detached tee delivery rate, measured on this machine** | §14.1. There is no coefficient. The mirror must be at least as fast as the thing feeding it, or it drops bytes — and any factor multiplied into that equation would be a factor chosen by looking at the candidates. |
| rehydrate | **≤ 5 ms** | A warm reattach must be invisible. One frame at 60 Hz is 16.7 ms, and the paint has to be serialized, relayed over a socket, and parsed by the front end inside it. Five milliseconds is the serializer's share — under a third of the frame. |
| cold paint | **≤ 5 ms** | As above; a checkpoint runs on a live session and may not stall it for a frame. |
| paint / sealed size | **≤ 2 MiB** | Geometry, not measurement. The restore window is 80 × 1000 = 80,000 cells. The heaviest screen a cell grid can hold changes style at every cell: a truecolour foreground (`CSI 38;2;R;G;B m`, ≤ 19 bytes) plus a 3-byte character = 22 bytes per cell ≈ 1.76 MB. 2 MiB is the ceiling over that worst case. |
| memory | **rss ≤ 32 MB** | §11.S **S6**: the sidecar mirrors every pane of a workspace and is a background service — sixteen panes inside 512 MB is the promise, so one mirror gets 32 MB. `heap` is reported but **is not a gate**: zero is a legitimate value for an engine that maps its own grid pages. |

**These budgets do not rank anyone.** They are a floor, and a floor has no first place. The
comparison table prints `ok` or `UNDER` against the floor and nothing else — the star that used
to crown the fastest unit is gone with the relative guard that made it mean something.

### 14.3 Standing — and how the floor proved it had teeth

The verdict is not a ratio. The gate holds a tee subscriber at **the unit's own measured feed
rate**, floods a real daemon, and reports what the daemon dropped. That is the harm the
requirement is about, so that is what the gate judges; `feed < demand` is the *explanation* for
the harm, not the finding.

On the reference machine (demand ≈ 90 MB/s, detached, real `soksak-ptyd`, 67 MB flood):

| unit | feed | vs. demand | **bytes the daemon dropped** | tail | fixtures |
| --- | --- | --- | --- | --- | --- |
| `soksak-sidecar-terminal-vt100` | 161 MB/s | ok | **0** | arrived | 7 / 7 |
| `soksak-sidecar-terminal-alacritty` | 152 MB/s | ok | **0** | arrived | 7 / 7 |
| `soksak-sidecar-terminal-wezterm` | 102 MB/s | ok | **0** | arrived | 7 / 7 |
| `soksak-sidecar-terminal-ghostty` | 93 MB/s | ok | **0** | arrived | 7 / 7 |

All four conform. That is the standing, but it is not the story — the story is what the floor
did on the way here.

**The floor caught a unit, and the unit moved.** When it was first derived, the wezterm unit fed
at **68 MB/s** against a demand of 84–89, and held at its own rate against a real daemon it
**lost 16.5 MB of a 67 MB flood**: with the app closed and a session dumping output, a quarter of
the screen it exists to restore never reached it. Under the old floor — 50 MB/s, read off the
candidates — it had passed comfortably, and that loss was invisible. It was recorded here as a
failure, and in the unit's own README, and the standard was not moved to accommodate it. The
mirror was made faster instead (68 → 102 MB/s), and against the same daemon at the same demand
it now drops nothing.

**The zeroes are the other half of the proof.** A floor nobody fails is not a floor, and a floor
everybody fails is not a requirement. This one failed exactly the unit that was losing data, and
passed exactly the units that were not — each of them held at its own rate against the same
daemon and dropping nothing. That is what says the line sits where it should.

Every other axis is clear for all four: the paint is 1.05 MB against a 2 MiB ceiling, rehydrate
and cold paint are half a millisecond against five, and the heaviest mirror holds 6 MB of the
32 MB it is allowed.

**Ghostty sits closest to the line** (93 against a demand that measures 84–90 across runs). It
drops nothing today. On a machine whose daemon is a little faster, it would — and that is worth
saying plainly rather than hiding behind a median.

## 15. The gate — where a verdict is actually made

A budget that is only checked when someone remembers to ask for it is not a budget, it is a
comment. The benchmark is `#[ignore]`d in the ordinary test run on purpose — it slows the
development loop and adds noise — so it would never have run on its own. What makes it binding
is that the verdict is not delivered by `cargo test` at all.

**A unit passes when `scripts/gate.sh` passes, and by no other means.** That script is the
whole judgement in one command: the seven fixtures against the declared goldens, the unit
tests, the real-daemon integration, and the performance budgets of §14.2 — every one of them
blocking. Nothing in it is optional and nothing in it can be skipped by forgetting.

**A unit's verdict is complete on its own.** It has to be: a judgement that needs the other
candidates in the room is a judgement the candidates have a hand in. The unit gate measures
the machine's demand itself (§14.1) and compares the unit to *that*, so it never needs to know
what anyone else scored. The fleet gate in this repo runs every unit's gate and prints one
table — it collects verdicts, it does not make them. The old relative guard ("no unit below a
quarter of the fastest in the same run") lived here and is deleted.

The gate also enforces the one rule the goldens cannot enforce about themselves: **no engine's
name may appear in a golden's reasoning** (`tests/goldens_cite_specs.rs`). A golden argued
from what an engine does is a golden that has an engine for an author, however carefully the
prose is worded.

Both gates were verified to fail when a budget is breached, not merely to pass when it is not.
