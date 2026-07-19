# Concurrent Node Insertion — Protocol Design

Status: design spec, not yet implemented. Target: `rust-developer`, via the `generic-rust`
develop → review loop.

Authority: Aspnes & Shah, "Skip Graphs" (`arXiv:cs/0306043`), Algorithm 2 (insert) and Algorithm 8
(fault-tolerant / backpointer repair). All algorithm content below is restated in this repo's own
notation and grounded in this codebase's actual types; no external document is cited by path.

## 0. Scope

In scope: the join/insert protocol for a **new node joining a Skip Graph that other insertions may be
happening in concurrently**, including the background repair mechanism that this document shows is
*required* (not optional) for insert correctness under concurrency.

Out of scope, explicitly:

- `search_by_mem_vec` (a separate, already-tracked future task; unrelated algorithm, unrelated source).
- Delete/leave.
- Node-failure fault tolerance (Algorithm 8 as used here repairs *ordering/backpointer* drift from
  concurrent inserts; surviving crashed/unreachable nodes is a different, future problem — the repair
  task designed below is a strict subset of Algorithm 8's full fault-tolerance scope).
- Real network transport / address resolution — everything below is expressed in terms of this
  codebase's existing `Identifier`-addressed `Network`/`Event` abstraction (`src/network/mod.rs`),
  which today is exercised only through the in-memory mock (`src/network/mock/`).
- Retroactively re-opening stage-2 height climbing on a node that already finished joining, if a
  stage-1 gap on one of its sides only gets filled in *after* that node's join completed. Section 5.3
  flags this as a known limitation and explicit follow-up, not solved here.

## 1. Background: the two-stage join algorithm (Algorithm 2)

A new node `u` joins in two stages:

- **Stage 1 (level 0).** `u` locates an existing node `s` — the largest currently-linked node with
  `s.key < u.key` — via a search, then asks `s` and `s`'s current right neighbor `z` to link `u` in
  between them at level 0.
- **Stage 2 (levels `ℓ > 0`, climbing).** Using its now-confirmed level-`(ℓ-1)` neighbors, `u` asks
  outward on each side whether that neighbor shares `u`'s membership-vector prefix through level `ℓ`.
  A match becomes `u`'s level-`ℓ` neighbor candidate; a non-match forwards the question further out.
  `u`'s height is the last level at which *either* side still had a match.

The single mechanism that makes both stages safe under concurrency, without any distributed lock, is
`change_neighbor` — restated precisely in Section 4. Section 5 shows, with a fully hand-traced
4-node example, that `change_neighbor` alone is **not** sufficient: it guarantees global ordering but
not backpointer consistency. Algorithm 8 (Section 6) closes that gap and must ship with insert, not
after it.

## 2. Message catalogue

All new payload structs live alongside the existing `IdSearchReq`/`IdSearchRes` convention
(`src/core/model/search.rs`); we recommend a new sibling module, e.g. `src/core/model/join.rs`, and
reuse the existing `Nonce` type for request/response correlation (it is no longer search-specific —
consider relocating it up to `src/core/model/mod.rs` as a small non-blocking cleanup).

Every request that can be **forwarded** (`GetLinkOp`, `BuddyOp`, `CheckNeighborOp`) carries the
originator's full `Identity` (not just `Identifier`) so that whichever node ultimately terminates the
chain can both install the entry in its own table *and* reply directly to the originator — exactly the
role `IdSearchReq::origin` plays for relayed searches today. The `nonce` is set once by the originator
and threaded unchanged through every hop.

`side`/`direction` fields are never re-interpreted hop-to-hop: `Direction::Right` always means "the
receiving node's own `right` slot" (nodes with larger keys), `Direction::Left` always means the
receiving node's own `left` slot — this is a global, receiver-owned concept (matching how
`ArrayLookupTable` already stores independent `left`/`right` vectors), not something relative to the
current hop or to the originator.

| Event variant | Payload | Direction | Fields |
|---|---|---|---|
| `GetMaxLevelOp` | `MaxLevelReq` | new node → introducer | `nonce`, `origin: Identifier` |
| `RetMaxLevelOp` | `MaxLevelRes` | introducer → new node | `nonce`, `max_level: LookupTableLevel` |
| `GetNeighborOp` | `NeighborReq` | `u` → `s` | `nonce`, `origin: Identifier`, `level`, `direction` |
| `RetNeighborOp` | `NeighborRes` | `s` → `u` | `nonce`, `level`, `direction`, `neighbor: Option<Identity>` |
| `GetLinkOp` | `LinkReq` | `u` → `s`/`z` (forwardable) | `nonce`, `candidate: Identity`, `side: Direction`, `level` |
| `SetLinkOp` | `LinkRes` | terminal node → `u` (also reused as an Algorithm-8 push correction) | `nonce`, `side: Direction`, `level`, `linked: Option<Identity>` |
| `BuddyOp` | `BuddyReq` | `u` → outward neighbor (forwardable) | `nonce`, `candidate: Identity`, `side: Direction`, `level` |
| `CheckNeighborOp` | `CheckNeighborReq` | periodic repair probe (forwardable) | `nonce`, `claimant: Identity`, `side: Direction`, `level` |

Notes on reuse, by design, to keep the catalogue minimal:

- `BuddyOp` has no dedicated response type: once a node along the chain matches `u`'s prefix, it
  simply runs the same accept-or-forward decision as `GetLinkOp` (Section 4) and the eventual
  terminal node replies with `SetLinkOp`. If the chain runs off the end of the list without a match,
  the last node replies `SetLinkOp{ level, side, linked: None }` — a definitive "no candidate on this
  side at this level," which `u`'s stage-2 loop treats as that side going permanently dry (Section
  3.3).
- `CheckNeighborOp`'s "notify the newly-linked node" correction reuses `SetLinkOp` (a push, not a
  reply to a pending request — applied through the same local `try_link`/`try_relink` machinery on
  receipt, see Section 4). Its "notify the evicted node" correction reuses `CheckNeighborOp` itself,
  recursively, with the relinking node as the new claimant (Section 6.2).

`Event` (`src/network/mod.rs`) gains all eight variants above alongside the existing
`TestMessage`/`SearchByIdRequest`/`SearchByIdResponse`; `SearchByIdRequest`/`SearchByIdResponse` are
reused **unmodified** for the Stage-1 introducer search (Section 3.2) — no changes needed there.

## 3. Join state machine

### 3.0 Architectural note: join orchestration is async

`Core::search_by_id` and `BaseNode::search_by_id` are synchronous today, correlating one relayed
request with one blocking channel receive. Join cannot use that pattern directly: Stage 1 has two
independent outstanding requests in flight at once (to `s` and to `z`), and Stage 2 has two independent
outstanding requests per level, for up to `LOOKUP_TABLE_LEVELS` (256) levels — spawning an OS thread per
outstanding request does not scale, and the periodic repair task (Section 6) requires `tokio::spawn`
regardless. The join orchestration entry point should therefore be `async`, correlating replies via
`tokio::oneshot` channels keyed by `Nonce` (the same shape as `BaseNode`'s existing
`request_id_map: Arc<Mutex<HashMap<Nonce, SyncSender<IdSearchRes>>>>`, generalized to the new message
types and made async-aware), with **every** wait wrapped in a timeout (`rust-standards.md` invariant
#8) — unlike `search_by_id`'s current unbounded blocking `recv()`, which this design does not
propagate to new code.

This is additive: it does not require retrofitting `search_by_id`.

### 3.1 Phase 0 — bootstrap

`u` is given (out of band, e.g. as a config value or CLI arg) the `Identity` of one already-joined
`introducer`.

1. `u → introducer`: `GetMaxLevelOp{ nonce, origin: u.id() }`.
2. `introducer → u`: `RetMaxLevelOp{ nonce, max_level }`, where `max_level` is the highest level at
   which the introducer has *any* populated entry — computable locally from the existing
   `LookupTable::left_neighbors()`/`right_neighbors()` (`max(...).unwrap_or(0)`), no new lookup-table
   method needed for this step.

`max_level` seeds the starting `level` for the Stage-1 search below. This is a latency optimization,
not a correctness requirement: `search_by_id`'s existing candidate-collection logic
(`src/node/core.rs`) already tolerates unpopulated levels by skipping `None` entries, so passing
`LOOKUP_TABLE_LEVELS - 1` unconditionally would also be correct, just marginally more wasteful to scan.

### 3.2 Phase 1 — Stage 1, level-0 linking

3. `u → introducer`: `SearchByIdRequest(IdSearchReq{ nonce, target: u.id(), origin: u.id(), level:
   max_level, direction: Direction::Right })` — **reuses the existing search machinery unmodified**,
   relayed exactly as today. `Direction::Right` search semantics ("greatest identifier ≤ target") is
   exactly "largest existing node with key < `u.id()`" once `u` isn't itself in the graph yet.
4. Terminal node replies `SearchByIdResponse(IdSearchRes{ ..., result: s_id })` directly to `u`. Call
   the node at `s_id` — `s`.

   *Edge case:* if the graph currently has exactly one node (the introducer itself, with an empty
   table), `search_by_id`'s existing fallback returns the introducer's own id — `s` = introducer. No
   special-casing needed; Stage 1 proceeds identically.

5. `u → s`: `GetNeighborOp{ nonce, origin: u.id(), level: 0, direction: Direction::Right }`.
6. `s → u`: `RetNeighborOp{ nonce, level: 0, direction: Direction::Right, neighbor }`, where `neighbor`
   is `s`'s current right-neighbor entry (`Option<Identity>`), possibly `None` if `s` currently
   believes itself the tail. Call this candidate `z` when present.

7. `u` now sends **two independent, concurrent** requests:
   - `u → s`: `GetLinkOp{ nonce, candidate: u_identity, side: Direction::Right, level: 0 }` — "install
     me as your right neighbor," i.e. `s` becomes `u`'s left neighbor.
   - `u → z` (only if `z` is `Some`): `GetLinkOp{ nonce, candidate: u_identity, side: Direction::Left,
     level: 0 }` — "install me as your left neighbor," i.e. `z` becomes `u`'s right neighbor.

   **Important asymmetry, stated explicitly because it is easy to get wrong:** the `s`-chain can only
   ever resolve `u`'s **left** neighbor (every hop it takes fills someone's `right` slot with `u`), and
   the `z`-chain can only ever resolve `u`'s **right** neighbor (every hop fills someone's `left` slot).
   They are not redundant attempts at the same slot; they resolve `u`'s two sides independently, and
   forwarding within *each* chain (Section 4) only protects against concurrent inserts landing within
   *that* chain's span — it does not cross over to fix the other side.

   Consequently: if `z` was `None` at query time but a concurrent insert lands to `u`'s right *before*
   or *while* `u`'s `s`-request is in flight, `u`'s right side is left **unresolved** (`None`) at the
   end of Stage 1. This is expected, not a bug — see Section 5.3, healed by Algorithm 8.

8. Each `GetLinkOp` is handled via `change_neighbor`/`try_link` (Section 4); the terminal accepting
   node replies `SetLinkOp{ nonce, side, level: 0, linked: Some(accepting_node's Identity) }` directly
   to `u`. `u` applies each reply to its own table via the same `try_link` primitive (Section 4) — there
   is exactly one code path in this design that ever writes a lookup-table entry, whether the write is
   `u` installing its own neighbor, a peer installing `u`, or a repair correction (Section 6).

Stage 1 is complete once `u` has resolved both sides (received a `SetLinkOp` for the `z`-request if one
was sent, or has recorded `None` immediately if no `z` was known) — `u` does **not** block Stage 2 on an
unresolved side; it proceeds with whatever it has.

### 3.3 Phase 2 — Stage 2, climbing

At level `ℓ`, starting `ℓ = 1`:

```
for each side S in {Left, Right}:
    if u.neighbor[S][ℓ-1] is None:
        # already dry from an earlier level — nothing to ask, stays None forever
        continue
    send BuddyOp{ nonce, candidate: u_identity, side: S, level: ℓ } to u.neighbor[S][ℓ-1]

wait for a resolution on every side queried this round (SetLinkOp, Some or None)

if every side is None at level ℓ (both the sides queried this round resolved None,
   and any side already dry from before remains None):
    u's climb stops; u's height = ℓ - 1
else:
    proceed to level ℓ + 1
```

A node `w` receiving `BuddyOp{ candidate, side, level: ℓ }`:

```
if w.mem_vec().common_prefix_bit(candidate.mem_vec()) >= ℓ:
    # w matches; runs the identical accept-or-forward decision GetLinkOp uses (Section 4),
    # with (level=ℓ, direction=side, candidate=candidate) — NOT an automatic accept, because a
    # different concurrent insertion may already occupy that slot.
else:
    if w.neighbor[side][ℓ-1] is Some(next):
        forward BuddyOp{ candidate, side, level: ℓ } to next   # walk further out, same direction
    else:
        reply SetLinkOp{ side, level: ℓ, linked: None } directly to candidate   # end of the line
```

Both sides are queried **concurrently** at each level (matching Stage 1's "independently" framing),
not sequentially — this affects latency, not message count.

Membership vectors in this codebase are already fully materialized upfront (`MembershipVector` is a
fixed 32-byte value generated once, not lazily extended bit-by-bit as an optimization in the paper).
`common_prefix_bit` is therefore a pure, lock-free local computation — no lazy-bit-generation logic is
needed anywhere in this design.

### 3.4 Every node is a full protocol participant from its first successful link

There is no "still joining" mode for *inbound* message handling. The instant `u` receives its first
`SetLinkOp` (i.e. some existing node has `try_link`-accepted `u`), `u` is reachable as a forward target
by other nodes' `GetLinkOp`/`BuddyOp`/`CheckNeighborOp` chains, exactly like any fully-joined node —
its handlers must already be live and correct. `process_incoming_event` additions for the eight new
variants are therefore stateless with respect to join progress; they always just consult the lookup
table via `try_link`/`try_relink`. The **only** join-specific state is the outward-facing orchestration
(Section 3.0's oneshot-correlated request tracking, and which level/side Stage 2 is currently waiting
on) — transient, owned by `u` alone, discarded once climbing stops.

## 4. Local atomicity: `try_link`/`try_relink`

### 4.1 `change_neighbor`, restated

Run by node `v` on receiving a link-request for candidate `u` on `side` at `level` (this is what both
`GetLinkOp` handling and a `BuddyOp` prefix match funnel into):

```
cmp = (side == Right) ? LessThan : GreaterThan
if v.neighbor[side][level] exists AND v.neighbor[side][level].key `cmp` u.key:
    # v's current neighbor on that side already sits strictly between v and u — not v's job
    forward the link-request to v.neighbor[side][level]
else:
    v.neighbor[side][level] = u
    reply to u confirming the link (SetLinkOp{ side, level, linked: v's identity })
```

The request moves monotonically toward `u`'s true position and self-corrects around concurrent
insertions landed in between, with no distributed lock. This is what guarantees the graph's *ordering*
invariant holds under arbitrary concurrent interleaving — but ordering is not the same guarantee as
backpointer consistency (Section 5).

### 4.2 Why `get_entry` + `update_entry` is not an implementation of this

`ArrayLookupTable::get_entry` and `::update_entry` (`src/core/lookup/array_lookup_table.rs`) each take
their own write/read lock independently — `get_entry` reads under one lock acquisition, `update_entry`
writes under a separate, later one. Implementing `change_neighbor` naively as "call `get_entry`, decide,
then call `update_entry`" reopens a race window between those two calls: two concurrently-arriving
`GetLinkOp`s for the *same* `(level, direction)` slot on the *same* node, handled by two async tasks,
can both read the same stale existing value, both independently decide "accept," and both call
`update_entry` — the second silently clobbers the first with **no forwarding ever having been
evaluated against the true post-first-write state**. This is a *local*, single-node instance of exactly
the same class of bug the 4-node example in Section 5 demonstrates across the network — a lost update
from an unprotected read-decide-write sequence. Two independently-locked calls cannot be composed into
one atomic decision; the compare-then-decide-then-write must happen inside a single lock acquisition.

### 4.3 Required lookup-table primitives

Two new methods, added to the `LookupTable` trait alongside the existing `get_entry`/`update_entry`/
`remove_entry`, each a **single write-lock critical section** covering the entire compare-then-decide
(matching this project's "one `RwLock` per logical entity" pattern — the whole compare-and-branch lives
inside one `inner.write()` critical section, never split across two lock acquisitions):

```
try_link(level, direction, candidate: Identity) -> LinkOutcome

LinkOutcome:
  LinkedDirectly                    # candidate was installed at (level, direction)
  Forward(existing: Identity)       # existing neighbor sits between self and candidate; table untouched

logic (inside one write-lock critical section):
  cmp = (direction == Right) ? LessThan : GreaterThan
  match current entry at (level, direction):
    Some(existing) where existing.id() `cmp` candidate.id() =>
        return Forward(existing)                          # table NOT modified
    _ =>
        set entry at (level, direction) = Some(candidate)
        return LinkedDirectly
```

```
try_relink(level, direction, claimant: Identity) -> RelinkOutcome

RelinkOutcome:
  AlreadyConsistent                          # current entry already equals claimant; no-op
  Forward(existing: Identity)                # existing sits between self and claimant; table untouched
  Relinked{ evicted: Option<Identity> }       # claimant installed; evicted is whatever was there before

logic (inside one write-lock critical section):
  cmp = (direction == Right) ? LessThan : GreaterThan
  match current entry at (level, direction):
    Some(existing) where existing == claimant =>
        return AlreadyConsistent
    Some(existing) where existing.id() `cmp` claimant.id() =>
        return Forward(existing)                          # table NOT modified
    other =>                                                # None, or existing on the far side of claimant
        evicted = other
        set entry at (level, direction) = Some(claimant)
        return Relinked{ evicted }
```

`try_link` backs `GetLinkOp` handling and a `BuddyOp` prefix match (Section 3). `try_relink` backs
`CheckNeighborOp` handling (Section 6). `try_relink`'s extra `AlreadyConsistent` outcome exists
specifically so a healthy, already-converged graph produces **zero** correction messages per repair
round — without it, every periodic tick would re-confirm every already-correct pointer forever, which
defeats the purpose of a low-overhead background task.

`ArrayLookupTable` should implement both directly against its existing `Arc<RwLock<InnerArrayLookupTable>>`
— no structural change to `InnerArrayLookupTable` (still just the two `Vec<Option<Identity>>`), just
two new methods whose entire body runs under one `inner.write()` guard.

Everything that ever mutates a lookup-table entry — `u` installing its own confirmed neighbors on
`SetLinkOp` receipt, a peer installing `u` on `GetLinkOp`/`BuddyOp` match, and every Algorithm-8
correction — funnels through `try_link` or `try_relink`. There is no second, unguarded write path.

## 5. The backpointer-consistency gap

### 5.1 Why `change_neighbor` alone is not enough

`change_neighbor`'s forwarding proof gives *ordering*: a node's pointers always point to something on
the correct side of it. It says nothing about whether the pointer on the *other end* points back. Two
concurrent, non-conflicting-looking insertions can each correctly maintain ordering at every step and
still leave the graph split into two connected fragments that never got stitched to each other.

### 5.2 Worked failure trace

Four nodes, `A < B < C < D`. `A` and `D` are already linked: `A.right[0] = D`, `D.left[0] = A`. `B` and
`C` join concurrently. Both search before either has linked in, so both independently discover `A` as
`s` and `D` as `z` (Section 3.2 steps 3–6). Both send their two `GetLinkOp`s (step 7):

- `B → A`: `GetLinkOp{ candidate: B, side: Right, level: 0 }`; `B → D`: `GetLinkOp{ candidate: B, side: Left, level: 0 }`.
- `C → A`: `GetLinkOp{ candidate: C, side: Right, level: 0 }`; `C → D`: `GetLinkOp{ candidate: C, side: Left, level: 0 }`.

**At `A`, `C`'s request is processed before `B`'s:**

1. `C` arrives: `A.right[0]` is `D`. `cmp = LessThan`; is `D < C`? No. → accept directly:
   `A.right[0] = C`. Reply `SetLinkOp{ linked: A }` to `C` ⇒ `C.left[0] = A`.
2. `B` arrives: `A.right[0]` is now `C`. Is `C < B`? No (`C > B`). → accept directly (again):
   `A.right[0] = B`, silently overwriting `C`'s entry. Reply `SetLinkOp{ linked: A }` to `B` ⇒
   `B.left[0] = A`. **No message is ever sent to `C` about this.**

**At `D`, `B`'s request is processed before `C`'s:**

3. `B` arrives: `D.left[0]` is `A`. `cmp = GreaterThan`; is `A > B`? No. → accept directly:
   `D.left[0] = B`. Reply `SetLinkOp{ linked: D }` to `B` ⇒ `B.right[0] = D`.
4. `C` arrives: `D.left[0]` is now `B`. Is `B > C`? No (`B < C`). → accept directly (again):
   `D.left[0] = C`, silently overwriting `B`'s entry. Reply `SetLinkOp{ linked: D }` to `C` ⇒
   `C.right[0] = D`. **No message is ever sent to `B` about this.**

**Resulting state:**

| Node | `left[0]` | `right[0]` | Consistent? |
|---|---|---|---|
| `A` | — | `B` | ✓ (`B.left[0] = A`) |
| `B` | `A` | `D` | `B.right[0]` is **stale** — should be `C` |
| `C` | `A` | `D` | `C.left[0]` is **stale** — should be `B` |
| `D` | `C` | — | ✓ (`C.right[0]` is stale-pointing at `D`, not the reverse) |

`(A, B)` and `(C, D)` are each internally consistent pairs. `B` and `C` are never linked to each other.
The level-0 ring is split into two fragments — `A↔B` and `C↔D` — purely from concurrent inserts, no
node failures involved, and every single `change_neighbor` decision along the way was locally correct
by its own rule. This is reachable, deterministic given this exact message ordering, and is exactly
what the required regression test (Section 7) reproduces.

### 5.3 A second gap category: a stale `RetNeighborOp` snapshot

Section 3.2's asymmetry note already flags this: if `z` (from `RetNeighborOp`) is `None` or stale at
the moment `u` samples it, but a third node concurrently links in on that side, `u`'s Stage 1 can finish
with a `None`/wrong entry on that side that neither of `u`'s two `GetLinkOp` chains can structurally
correct (the `s`-chain only ever resolves `u`'s left side; the `z`-chain only ever resolves the right).
This is a different root cause from Section 5.2's clobber but the same shape of defect — a missing or
stale reverse pointer — and it heals through the identical Algorithm 8 mechanism (Section 6), via the
*other* node's own periodic sweep eventually discovering the mismatch and correcting it, without `u`
needing to have gotten it right initially.

Known limitation, explicitly out of scope here: if `u`'s stage-2 height climb on that side has already
terminated (recorded permanent `None`) by the time Algorithm 8 fills in the level-0 gap, `u` does not
retroactively reopen climbing on that side. Flagged as follow-up work, not solved by this design.

## 6. Algorithm 8 — continuous backpointer repair

**This ships together with insert, as a continuously-running background task from the moment a node
joins — not as a later "fault tolerance" feature.** Section 5.2 does not involve any node failure; the
split is reachable purely from concurrent, individually-correct inserts. Without this task running,
concurrent joins are incorrect (in the backpointer-consistency sense), independent of whether any node
ever crashes.

### 6.1 The periodic sweep

Every tick, for **every** currently-populated `(level, direction)` entry in its own table — enumerated
via the existing `LookupTable::left_neighbors()`/`right_neighbors()`, no new enumeration method needed
— a node `v` unconditionally sends one probe to that neighbor `w`:

```
for (level, w) in v.left_neighbors():
    send CheckNeighborOp{ claimant: v_identity, side: Right, level } to w   # "is your Right slot me?"
for (level, w) in v.right_neighbors():
    send CheckNeighborOp{ claimant: v_identity, side: Left, level } to w    # "is your Left slot me?"
```

The check is unconditional and embodied entirely in the round trip — `v` does not attempt to
pre-guess staleness locally (it has no way to know `w`'s state); `w`'s handling of the probe is what
determines whether anything was actually wrong.

### 6.2 `check_neighbor`, restated precisely

Run by node `w` on receiving `CheckNeighborOp{ claimant, side, level }` (structurally the same
accept-or-forward shape as `change_neighbor`, backed by `try_relink` instead of `try_link`):

```
cmp = (side == Right) ? LessThan : GreaterThan
match w.neighbor[side][level]:
    Some(existing) where existing == claimant:
        # already consistent — nothing to do, no message sent
    Some(existing) where existing.key `cmp` claimant.key:
        # existing sits strictly between w and claimant — not w's job, walk outward
        forward CheckNeighborOp{ claimant, side, level } to existing
    other:  # None, or existing is on the far side of claimant (claimant is actually closer to w)
        evicted = other
        w.neighbor[side][level] = claimant          # relink, via try_relink's Relinked{evicted} arm
        # fire two correction messages:
        send SetLinkOp{ side: opposite(side), level, linked: Some(w's identity) } to claimant
            # tells claimant its reciprocal pointer should be w — applied at claimant via try_link
        if evicted is Some(old):
            send CheckNeighborOp{ claimant: w's identity, side: opposite(side), level } to old
                # re-probes the evicted node's relationship to w using the identical mechanism,
                # recursively — no third message type needed
```

`try_relink`'s `AlreadyConsistent`/`Forward`/`Relinked` outcomes (Section 4.3) map directly onto the
three branches above, each a single write-lock critical section.

### 6.3 Worked healing trace

Continuing Section 5.2's end state. Say `B`'s own periodic sweep runs first (the paper's guarantee
holds regardless of which of `B` or `C` triggers first):

- `B`'s sweep on its `right[0] = D` entry sends `CheckNeighborOp{ claimant: B, side: Left, level: 0 }`
  to `D` ("is your left neighbor me?").
- **At `D`:** `D.left[0] = C`. `cmp = GreaterThan`; is `C > B`? Yes → `C` sits strictly between `D` and
  `B` → forward to `C`.
- **At `C`:** same probe, unchanged (`claimant: B, side: Left, level: 0`). `C.left[0] = A`. Is `A > B`?
  No (`A < B`) → not strictly between → **relink**: `C.left[0] = B` (`C`'s stale pointer is now fixed).
  Evicted = `A`. Fires:
  - `SetLinkOp{ side: Right, level: 0, linked: C }` → `B`. `B` applies it: `B.right[0] = C` (`B`'s
    stale pointer is now fixed — **both stale pointers healed after this single triggering sweep**).
  - `CheckNeighborOp{ claimant: C, side: Right, level: 0 }` → `A` (the evicted node). At `A`:
    `A.right[0] = B` already; `cmp = LessThan`; is `B < C`? Yes → forward to `B`. At `B`:
    `B.right[0]` is now `C` (just fixed above) → already matches claimant `C` → no-op. Converges, no
    further messages.

Final state: `A.right = B`, `B.left = A`, `B.right = C`, `C.left = B`, `C.right = D`, `D.left = C` — the
full `A↔B↔C↔D` chain, fully reciprocal, no lost connectivity, healed by **one round** triggered by a
single node's sweep.

### 6.4 Task lifecycle

- Spawned via `tokio::spawn` when a node becomes active (e.g. from `BaseNode::new` or an explicit
  `start()`), against a child of the node's existing `IrrevocableContext` (`src/core/context/mod.rs`,
  currently unwired into any node/network code — this is its first real use) via `ctx.child(...)`, so
  the repair task is cancelled automatically if the node's parent context is cancelled, and can also be
  cancelled independently.
- The returned `JoinHandle` is stored on `BaseNode` and aborted on shutdown — RAII, no orphaned task
  surviving node teardown (`rust-standards.md` invariant #7).
- **The check interval sits behind a trait, never a literal `tokio::time::sleep`/`interval` call
  in the task body.** Something shaped like a `RepairSchedule` trait with an async
  "wait for the next tick" method; the production implementation wraps `tokio::time::interval`, and a
  test implementation is driven by an explicit, manually-fired gate (e.g. a `tokio::sync::Notify` or a
  bounded channel the test controls), so a test can trigger **exactly one** sweep deterministically and
  await its completion under a timeout, with zero wall-clock waiting (`rust-standards.md` invariant #8;
  this project's existing tests never sleep-and-hope, e.g. `join_with_timeout` /
  `join_all_with_timeout` in the test fixtures already establish this pattern for thread joins — the
  same discipline applies here for the async task).

## 7. Required regression test

A **deterministic** test reproducing Section 5.2's exact interleaving and Section 6.3's healing round —
not a randomized/threaded stress test hoping to hit the race. This codebase's mock network
(`NetworkHub`/`MockNetwork`, `src/network/mock/`) dispatches synchronously and recursively (`send_event`
directly calls the target's registered `MessageProcessor` inline, in the caller's own call stack), so
the exact message arrival order at each node is fully controlled by the literal order in which the test
issues calls — no thread races, no timing assumptions.

**Setup:** four nodes `A < B < C < D` (reuse the existing sorted-identifier/`NetworkHub`/`MockNetwork`
wiring pattern already established in `src/node/skip_graph_integration_test.rs`'s `LocalSkipGraph`
fixture). Pre-wire level 0 only: `A.right[0] = D`, `D.left[0] = A` (direct `LookupTable::update_entry`
calls, as existing tests already do — no need to run a real join to set up the pre-state). Higher
levels are irrelevant to this scenario and can stay empty.

**Adversarial ordering (drive this by calling each node's message-handling entry point directly, in
this exact sequence, from a single test thread — not via `std::thread::spawn`):**

1. Deliver `C`'s `GetLinkOp{ candidate: C, side: Right, level: 0 }` to `A`.
2. Deliver `B`'s `GetLinkOp{ candidate: B, side: Right, level: 0 }` to `A`.
3. Deliver `B`'s `GetLinkOp{ candidate: B, side: Left, level: 0 }` to `D`.
4. Deliver `C`'s `GetLinkOp{ candidate: C, side: Left, level: 0 }` to `D`.
5. Apply each resulting `SetLinkOp` reply to the requester's own table (either by feeding it through
   the requester's own handler, or, equivalently, by directly recording the known reply value — the
   point under test is the receiver-side `A`/`D` behavior, not the requester-side apply step).

**Assertion 1 (transient stale state is reachable):**

```
A.right[0] == B     B.left[0] == A     B.right[0] == D   // stale
D.left[0]  == C     C.right[0] == D    C.left[0]  == A   // stale
```

**Then, trigger exactly one repair round** — preferably by exercising the real spawned repair task
(Section 6.4) gated through the test-controlled `RepairSchedule`, firing one tick and awaiting
completion under a bounded timeout; a direct call to the `CheckNeighborOp` handler with the specific
probe from Section 6.3 is an acceptable simpler fallback if wiring the full task proves awkward for a
first pass, but the task-based path is preferred since it exercises the actual shipped code, not a
hand-picked shortcut through it.

**Assertion 2 (one round fully heals both stale pointers, no lost connectivity):**

```
A.right[0] == B     B.left[0] == A
B.right[0] == C     C.left[0] == B
C.right[0] == D     D.left[0] == C
```

i.e. the complete, correctly-reciprocal `A ↔ B ↔ C ↔ D` chain.

Colocate as a sibling `*_test.rs` (this project's existing convention, e.g. adjacent to wherever the
new join/repair logic lands), not a `#[cfg(test)] mod tests` block.

## 8. Do not implement a pessimistic, lock-based concurrency model

A distributed-lock-based approach to concurrent insertion (per-node mutual-exclusion locks plus a
version counter to detect stale reads) is **not** what Aspnes & Shah's Algorithm 2 specifies, and is
explicitly rejected for this design — not merely as a style preference. Do not introduce a lock/version
type, a "locked" node state, or any cross-node mutual-exclusion protocol anywhere in this feature.

The lock-free design in this document (`change_neighbor`/`try_link` for forward progress,
`check_neighbor`/`try_relink` for convergence) is the paper's actual mechanism, is proven correct for
ordering under arbitrary interleaving without coordination, and Section 6 closes the one gap it leaves
open. It is strictly simpler than a distributed lock: no lock acquisition failure modes, no lock
timeout/retry policy, no risk of a stuck lock outliving a crashed holder.

## 9. Implementation surface (for `rust-developer`)

Not exhaustive Rust code (this is a spec) — the concrete surface this design implies:

- `src/network/mod.rs`: `Event` gains the eight variants in Section 2.
- New payload module (e.g. `src/core/model/join.rs`): the eight request/response structs in Section 2;
  consider relocating `Nonce` out of `search.rs` to a shared location.
- `src/core/lookup/mod.rs` + `src/core/lookup/array_lookup_table.rs`: `LookupTable` trait gains
  `try_link`/`try_relink` (Section 4.3); `ArrayLookupTable` implements both as single write-lock
  critical sections on its existing `Inner`.
- `src/node/core.rs`: `Core` gains pure-local decision methods with no network awareness, mirroring how
  `search_by_id` is pure-local today — e.g. a `GetLinkOp`/`BuddyOp`-match handler wrapping `try_link`, a
  `CheckNeighborOp` handler wrapping `try_relink`, a prefix-match predicate wrapping
  `MembershipVector::common_prefix_bit`, and a "my current max populated level" query wrapping
  `left_neighbors()`/`right_neighbors()`.
- `src/node/base_node.rs`: an async join orchestration entry point (Section 3.0-3.3), new
  `process_incoming_event` match arms for all eight variants, and the spawned repair task (Section 6.4)
  with its `RepairSchedule` trait and stored, RAII-aborted `JoinHandle`.
- New sibling test file(s) for Section 7's regression test, following the `*_test.rs` convention.

## 10. Non-goals recap

`search_by_mem_vec`, delete/leave, node-failure fault tolerance beyond backpointer repair, real network
transport, and retroactive stage-2 re-climb after a late repair (Section 5.3) are all explicitly not
addressed by this document.
