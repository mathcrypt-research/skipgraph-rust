//! request/response payloads for the join (insert) protocol and its background
//! backpointer-repair mechanism (see `docs/protocol/concurrent-insert.md`).
//!
//! every `side`/`direction` field is receiver-owned and never re-interpreted
//! hop-to-hop: `Direction::Right` always means the receiving node's own right
//! slot (nodes with larger identifiers), `Direction::Left` always the receiving
//! node's own left slot — a global concept, not relative to the current hop or
//! the originator.
//!
//! forwardable requests ([`LinkReq`], [`BuddyReq`], [`CheckNeighborReq`]) carry
//! the originator's/claimant's full [`Identity`] so the node terminating the
//! chain can install the entry in its own table and reply directly to the
//! originator. the `nonce` is set once by the originator and threaded unchanged
//! through every hop.

use crate::core::lookup::LookupTableLevel;
use crate::core::model::direction::Direction;
use crate::core::model::identity::Identity;
use crate::core::model::search::Nonce;
use crate::core::Identifier;

/// asks the introducer for the highest level at which it has any populated
/// lookup-table entry, seeding the joining node's stage-1 search level.
#[derive(Debug, Copy, Clone)]
pub struct MaxLevelReq {
    /// correlation nonce, set once by the originator and threaded unchanged through every hop.
    pub nonce: Nonce,
    /// the identifier of the joining node that initiated the request.
    pub origin: Identifier,
}

/// the introducer's reply to [`MaxLevelReq`].
#[derive(Debug, Copy, Clone)]
pub struct MaxLevelRes {
    /// correlation nonce, set once by the originator and threaded unchanged through every hop.
    pub nonce: Nonce,
    /// the highest level at which the responder has any populated lookup-table entry.
    pub max_level: LookupTableLevel,
}

/// asks a node for its current neighbor entry at a given level and direction.
#[derive(Debug, Copy, Clone)]
pub struct NeighborReq {
    /// correlation nonce, set once by the originator and threaded unchanged through every hop.
    pub nonce: Nonce,
    /// the identifier of the node that initiated the request.
    pub origin: Identifier,
    /// the lookup-table level being queried.
    pub level: LookupTableLevel,
    /// receiver-owned direction: `Direction::Right` always means the receiving node's own
    /// right slot (nodes with larger identifiers), `Direction::Left` its own left slot —
    /// never re-interpreted hop-to-hop, not relative to the current hop or the originator.
    pub direction: Direction,
}

/// the reply to [`NeighborReq`], carrying the queried neighbor entry, if any.
#[derive(Debug, Copy, Clone)]
pub struct NeighborRes {
    /// correlation nonce, set once by the originator and threaded unchanged through every hop.
    pub nonce: Nonce,
    /// the lookup-table level that was queried.
    pub level: LookupTableLevel,
    /// receiver-owned direction: `Direction::Right` always means the responding node's own
    /// right slot (nodes with larger identifiers), `Direction::Left` its own left slot —
    /// never re-interpreted hop-to-hop, not relative to the current hop or the originator.
    pub direction: Direction,
    /// the responder's neighbor entry at the queried level and direction, if populated.
    pub neighbor: Option<Identity>,
}

/// asks the receiver to adopt the candidate as its neighbor on the given side and
/// level. the request is forwardable: a receiver with an existing neighbor sitting
/// between itself and the candidate passes the request along instead of linking, so
/// it may travel several hops before some node installs the candidate. that node
/// replies directly to the candidate with a [`LinkRes`], which is why the
/// candidate's full [`Identity`] travels in the request.
#[derive(Debug, Copy, Clone)]
pub struct LinkReq {
    /// correlation nonce, set once by the originator and threaded unchanged through every hop.
    pub nonce: Nonce,
    /// the full identity of the joining node proposing itself as the receiver's
    /// neighbor at this side and level; the receiver either installs it in its own
    /// table or forwards the request when an existing neighbor sits between them.
    pub candidate: Identity,
    /// receiver-owned side: `Direction::Right` always means the receiving node's own
    /// right slot (nodes with larger identifiers), `Direction::Left` its own left slot —
    /// never re-interpreted hop-to-hop, not relative to the current hop or the originator.
    pub side: Direction,
    /// the lookup-table level at which the link is requested.
    pub level: LookupTableLevel,
}

/// reports how a link request ended: `linked` names the node that adopted the
/// candidate as its neighbor, or is `None` when the forwarding chain ran out
/// without finding anyone to link. besides answering [`LinkReq`] and [`BuddyReq`],
/// this message also arrives unsolicited during backpointer repair, when a node
/// pushes a correction telling the recipient to update its own pointer — so a
/// receiver must not assume it matches a pending request.
#[derive(Debug, Copy, Clone)]
pub struct LinkRes {
    /// correlation nonce, set once by the originator and threaded unchanged through every hop.
    pub nonce: Nonce,
    /// receiver-owned side: `Direction::Right` always means the receiving node's own
    /// right slot (nodes with larger identifiers), `Direction::Left` its own left slot —
    /// never re-interpreted hop-to-hop, not relative to the current hop or the originator.
    pub side: Direction,
    /// the lookup-table level the link decision applies to.
    pub level: LookupTableLevel,
    /// the identity of the node that installed the candidate in its own table —
    /// from the candidate's point of view, its new neighbor at this side and level;
    /// `None` means no candidate exists on this side at this level.
    pub linked: Option<Identity>,
}

/// searches for the candidate's "buddy": the nearest node on the given side whose
/// membership vector shares at least `level` prefix bits with the candidate — that
/// is, the candidate's neighbor-to-be at `level`. the name follows the `buddyOp`
/// message of the skip graphs paper. unlike [`NeighborReq`] (a read-only query
/// answered in place), this is a search: each receiver that fails the prefix check
/// forwards the request outward along its links at the level below, and the first
/// node that passes it installs the candidate in its own table and replies directly
/// with a [`LinkRes`]. there is no dedicated response type: `linked: None` in the
/// [`LinkRes`] means the chain ran out — no buddy exists on this side at this level.
#[derive(Debug, Copy, Clone)]
pub struct BuddyReq {
    /// correlation nonce, set once by the originator and threaded unchanged through every hop.
    pub nonce: Nonce,
    /// the full identity of the joining node seeking a buddy at this level.
    pub candidate: Identity,
    /// receiver-owned side: `Direction::Right` always means the receiving node's own
    /// right slot (nodes with larger identifiers), `Direction::Left` its own left slot —
    /// never re-interpreted hop-to-hop, not relative to the current hop or the originator.
    pub side: Direction,
    /// the lookup-table level at which a neighbor is sought.
    pub level: LookupTableLevel,
}

/// a periodic repair probe asking the receiver "does your `side` slot at `level`
/// point back at the claimant?" — sent by a node sweeping its own table to verify
/// that each of its neighbors holds the reciprocal pointer. the probe is
/// forwardable: a receiver whose slot holds a node sitting strictly between itself
/// and the claimant passes the probe to that node instead, so it may travel several
/// hops. the node where it stops either finds the pointer already correct (no-op)
/// or relinks the slot to the claimant and pushes a [`LinkRes`] correction back to
/// it — which is why the claimant's full [`Identity`] travels in the probe.
#[derive(Debug, Copy, Clone)]
pub struct CheckNeighborReq {
    /// correlation nonce, set once by the originator and threaded unchanged through every hop.
    pub nonce: Nonce,
    /// the full identity of the node claiming to be the receiver's neighbor.
    pub claimant: Identity,
    /// receiver-owned side: `Direction::Right` always means the receiving node's own
    /// right slot (nodes with larger identifiers), `Direction::Left` its own left slot —
    /// never re-interpreted hop-to-hop, not relative to the current hop or the originator.
    pub side: Direction,
    /// the lookup-table level being checked.
    pub level: LookupTableLevel,
}
