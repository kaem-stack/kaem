# kaem sandbox — plan

A Packet Tracer-style sandbox for the kaem mesh-radio stack. You spawn nodes in a
simulated RF space, position them, pair them (key exchange), and drive each one
from a Cisco-IOS-style console. The protocol logic lives in library crates; the
sandbox is the harness that instantiates many nodes, owns the virtual clock and
the radio medium, and routes signals between them per a propagation model.

No SDR required — and now, no second terminal either. One process, N nodes, full
control.

## Mental model (Packet Tracer → kaem)

| Packet Tracer            | kaem sandbox                                           |
| ------------------------ | ------------------------------------------------------ |
| Workspace / canvas       | 2D RF field (`Medium` holds positions)                 |
| Drop a device            | `node add alice 10 20`                                 |
| Wire two devices         | nothing — RF reachability is computed from distance    |
| Click device → IOS CLI   | `attach alice` → node console                          |
| Realtime / Simulation    | `run` (wall-clock) / `step [n]` (deterministic)        |
| Save/open `.pkt`         | `save topo.json` / `load topo.json`                    |

The faithful part: each sim node runs the **real modem** from `kaem-radio`. The
only swapped piece is the `Channel` — instead of UDP it's an in-process
`SimChannel` that hands IQ to the `Medium`, which attenuates/drops by distance
and delivers to in-range receivers. Same signal path the real hardware will use.

## Workspace reorg (Phase 0)

Split libraries from apps, as requested:

```
crates/                 # libraries (no main)
  kaem-transport        # the port (unchanged)
  kaem-codec            # wire protocol (gets pairing message variants)
  kaem-crypto           # ML-KEM-768 + ChaCha20 (gets wired in)
  kaem-radio            # modem + Channel seam (made channel-generic)
  kaem-udp              # dev adapter (unchanged)
  kaem-loopback         # dev adapter (unchanged)
  kaem-node     [NEW]   # the steppable, I/O-free node core — "the logic"
  kaem-sim      [NEW]   # the Medium, propagation, SimChannel, clock
binaries/               # apps (have main)
  kaem                  # the existing TUI chat app (moves here)
  kaem-sandbox  [NEW]   # the sandbox + operator CLI
```

Root `Cargo.toml`: `members = ["crates/*", "binaries/*"]` and update the
`[workspace.dependencies]` `path = ...` entries to the new locations. This phase
is pure plumbing — no behavior change — so land it first and confirm `cargo
build && cargo test` stay green before anything else.

## Dependency graph (new crates)

- `kaem-node` → `kaem-codec`, `kaem-crypto`. Deals in **byte frames**; owns no
  socket, no terminal, no wall clock. Time is passed in.
- `kaem-sim` → `kaem-radio` (Channel + Iq), `rand` (seeded loss), `serde`/`serde_json`
  (topology save/load).
- `kaem-sandbox` (bin) → `kaem-node`, `kaem-sim`, `kaem-radio`, `kaem-codec`,
  `kaem-crypto`. Owns the REPL.

## Phase 1 — Extract the steppable node core (`kaem-node`)

Move `core/chat.rs`, `core/model.rs`, `core/seed.rs` out of the `kaem` binary
into `kaem-node`. Refactor so the node neither owns its transport loop nor calls
`Utc::now()` — both get injected. Proposed surface:

```rust
pub struct Node { /* callsign, contacts, keypair, known peer pubkeys, log, ... */ }

pub enum Command {            // operator/console intents
    Send { to: String, body: String },
    Pair { peer: String },    // begin pairing handshake with a peer
    // ...
}

pub struct Outbound(pub Vec<u8>);   // a frame the node wants to transmit

impl Node {
    pub fn new(callsign: String) -> Self;          // generates an ML-KEM keypair
    pub fn command(&mut self, cmd: Command, now: Time) -> Vec<Outbound>;
    pub fn on_frame(&mut self, frame: &[u8], now: Time);   // a frame arrived
    pub fn tick(&mut self, now: Time) -> Vec<Outbound>;    // timeouts/retransmits
    // read-only views for the console:
    pub fn peers(&self) -> &[Peer];
    pub fn log(&self) -> &[LogLine];
    pub fn public_key(&self) -> &[u8];
}
```

`Time` is virtual (e.g. `u64` ms). Rewire the `kaem` TUI binary to drive this
core: keyboard → `Command`, `RadioTransport::recv` → `on_frame`, drain
`Outbound` → `RadioTransport::send`. The TUI must keep working — it's the
regression guard that proves the core is right. **Payoff:** the same core powers
both the live app and the sandbox, and is finally unit-testable.

## Phase 2 — Channel-generic radio + the medium (`kaem-sim`)

1. **Expose the seam in `kaem-radio`:** `pub use channel::Channel;` and export
   `Iq`; make `RadioTransport` hold a `Box<dyn Channel>` (keep `::bind` as the
   UDP convenience constructor). Now any `Channel` can back the same modem.
2. **`Medium`:** registry of `{ node_id, position }`; `transmit(node_id, &[Iq],
   now)` computes, for every other node, distance → reachable? → enqueue
   (optionally attenuated / dropped) IQ into that node's inbox.
   - v1 propagation: unit-disc range radius `R` + Bernoulli loss `p`, seeded RNG
     (deterministic). Nothing more.
   - later, only if a question demands it: free-space path loss → SNR → drop
     below floor; propagation delay; collisions when transmissions overlap.
3. **`SimChannel`:** implements `Channel`, holds a handle to the shared `Medium`
   (single-threaded → `Rc<RefCell<Medium>>`) plus its own `node_id`. `transmit`
   pushes to the medium; `receive` pops this node's delivered IQ.

A sim node is then: `kaem-node::Node` + `RadioTransport::new(modem, SimChannel)`.
The real modem and real propagation run in-process, deterministically.

## Phase 3 — Pairing via `kaem-crypto`

The crypto crate already does hybrid per-message sealing: `keys::generate()`
mints an ML-KEM-768 keypair; `crypto::encrypt(cfg, peer_pubkey, body)` seals a
message that only the peer's secret key opens. So **pairing = authenticated
public-key exchange**, not a separate session handshake.

- Each `Node` generates a keypair on creation.
- Add `WireMessage` variants (or a tagged frame kind) to `kaem-codec`:
  `PairHello { from, pubkey }` and `PairAck { from, pubkey }`.
- `pair alice bob` (sandbox) or `pair bob` (node console) makes alice emit a
  `PairHello`; bob records alice's pubkey and replies `PairAck`; alice records
  bob's. Both now hold the other's public key = "paired."
- After pairing, chat bodies to that peer are sealed with `crypto::encrypt`
  before `encode`, and opened with the node's secret key on receipt. The
  `encrypted` flag already on `Chat` drives the UI/console indicator.

## Phase 4 — The sandbox binary + operator CLI (`kaem-sandbox`)

A single process owning: the `Medium`, a `name → (Node, RadioTransport over
SimChannel)` map, and the virtual clock. A two-scope REPL, Cisco-style.

**Sandbox scope:**
```
node add <name> [x y]      spawn a node (random position if omitted)
node move <name> <x> <y>   reposition
node del <name>            remove
nodes                      list nodes, positions, paired peers
links                      show who can currently hear whom
range <meters>             set reachability radius
loss <0..1>                set per-link loss probability
seed <n>                   set RNG seed (reproducible runs)
pair <a> <b>               run the pairing handshake between two nodes
step [n]                   advance n ticks deterministically, print events
run / pause                advance against the wall clock until paused
time                       show virtual time
save <file> / load <file>  serialize/restore topology (serde_json)
attach <name>              drop into a node's console
help / quit
```

**Node scope (after `attach alice`, prompt `alice>`):**
```
send <to> <text>     queue a message (sealed if paired)
pair <peer>          begin pairing with a peer
peers                paired peers + their key fingerprints
log                  this node's message/event log
pos                  show position
exit                 back to sandbox scope
```

Time model: `step`/`step n` is deterministic and single-threaded (Packet
Tracer's *simulation* mode) — advance the clock, `tick` every node, route the
medium, print what moved. `run` is the *realtime* mode: a timer-driven loop
doing the same until `pause`. REPL line editing via `rustyline` (history) or
plain stdin to stay zero-dep — pick during Phase 4.

## Phase 5 (optional) — ratatui field view

A 2D amber/gray view of node positions, live links, and packet animation,
matching the kaem design language (minimal, brutalist-edge, no emojis). Toggle
from the sandbox. Defer until the CLI sandbox is solid.

## Decisions & risks

- **Single-threaded sim.** `Rc<RefCell<Medium>>`, no threads → determinism is
  free and step mode is exact. `run` mode interleaves via a timer; it is not
  meant to be reproducible (step mode is).
- **IQ-level routing, not frame-level.** The medium moves `Vec<Iq>` so the modem
  stays honest end to end. It's barely more code than frame routing and far more
  faithful — this is the whole point of having our own modem.
- **Keep the TUI app alive** through the `kaem-node` extraction; it's the proof
  the core didn't regress.
- **Scope discipline.** Range radius + loss prob first. No path loss, fading, or
  collisions until a concrete protocol question needs them. Do not rebuild ns-3.
- **Phase 0 lands alone.** Reorg is noisy in diffs; isolate it so later phases
  review cleanly.

## Suggested commit sequence

1. `refactor(workspace): split into crates/ (libs) and binaries/ (apps)`
2. `refactor(node): extract steppable Node core into kaem-node`
3. `feat(radio): make RadioTransport channel-generic; expose Channel`
4. `feat(sim): add Medium + SimChannel with range/loss propagation`
5. `feat(crypto): pair nodes by exchanging ML-KEM public keys`
6. `feat(sandbox): operator CLI to spawn, place, pair, and drive nodes`
