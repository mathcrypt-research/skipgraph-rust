use crate::core::model::direction::Direction;
use crate::core::model::identity::Identity;

pub mod array_lookup_table;
mod array_lookup_table_test;

/// LookupTableLevel represents level of a lookup table. entry in the table.
pub type LookupTableLevel = usize;

/// Outcome of a [`LookupTable::try_link`] compare-then-act decision.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum LinkOutcome {
    /// The candidate was inserted at the requested `(level, direction)` slot: the slot was
    /// empty, or its previous occupant did not sit strictly between this node and the
    /// candidate on that side, so the candidate is now the entry there.
    LinkedDirectly,
    /// The table was left untouched: the carried identity is the existing entry at
    /// `(level, direction)`, and it sits strictly between this node and the candidate on that
    /// side — it is closer to the candidate's true position, so the link request belongs there
    /// instead.
    Forward(Identity),
}

/// Outcome of a [`LookupTable::try_relink`] compare-then-act decision — the repair counterpart
/// of [`LinkOutcome`], returned when a node (the "claimant") checks or re-asserts a link it
/// believes it should already hold, rather than requesting a brand-new one.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum RelinkOutcome {
    /// The slot already points to the claimant, so the pointer was never actually wrong:
    /// nothing is written and no correction is needed. `try_link` has no equivalent outcome,
    /// since a first-time candidate is never already installed.
    AlreadyConsistent,
    /// The table was left untouched: the carried identity is the existing entry at
    /// `(level, direction)`, and it sits strictly between this node and the claimant on that
    /// side — it is closer to the claimant's true position, so it is not this node's call to
    /// make, and the repair check should be retried against that neighbor instead.
    Forward(Identity),
    /// The slot was empty, or its occupant was neither the claimant nor strictly between this
    /// node and the claimant (the same comparison `try_link` uses): the claimant is installed
    /// as the new entry, evicting whatever occupied the slot before (`evicted` is `None` if it
    /// was empty).
    Relinked { evicted: Option<Identity> },
}

/// LookupTable is the core view of Skip Graph node towards the network.
#[unimock::unimock(api = LookupTableMock)]
pub trait LookupTable: Send + Sync {
    /// Update the entry at the given level and direction.
    fn update_entry(
        &self,
        identity: Identity,
        level: LookupTableLevel,
        direction: Direction,
    ) -> anyhow::Result<()>;

    /// Remove the entry at the given level and direction.
    fn remove_entry(&self, level: LookupTableLevel, direction: Direction) -> anyhow::Result<()>;

    /// Get the entry at the given level and direction.
    /// Returns None if the entry is not present.
    /// Returns Some(Identity) if the entry is present.
    fn get_entry(
        &self,
        level: LookupTableLevel,
        direction: Direction,
    ) -> anyhow::Result<Option<Identity>>;

    /// Atomically decides whether `candidate` becomes the neighbor at `(level, direction)`, or
    /// whether an existing neighbor there already sits strictly between this node and
    /// `candidate` on that side and the request should be forwarded to it instead.
    ///
    /// The decision is atomic: inspecting the current entry and, when accepting, inserting
    /// `candidate` happen as one indivisible step with respect to any other concurrent call on
    /// the same `(level, direction)` slot — no caller can observe or race a partial decision.
    ///
    /// The `direction` parameter is receiver-owned, never re-interpreted hop-to-hop: it always names this
    /// node's own slot (`Direction::Right` is this node's own right slot, holding neighbors with
    /// larger identifiers; `Direction::Left` is its own left slot), never something relative to a
    /// caller or hop. An existing entry sits strictly between this node and `candidate` when,
    /// for `Direction::Right`, `existing.id() < candidate.id()`; for `Direction::Left`,
    /// `existing.id() > candidate.id()`:
    ///
    /// - if it does: this node's own entry at `(level, direction)` is left unchanged, and
    ///   [`LinkOutcome::Forward`] carrying that existing neighbor is returned — the caller
    ///   should retry the link request against that neighbor instead.
    /// - otherwise (the slot is empty, or the existing entry does not sit strictly between):
    ///   `candidate` is inserted into this node's own entry at `(level, direction)`, and
    ///   [`LinkOutcome::LinkedDirectly`] is returned.
    ///
    /// # Preconditions
    ///
    /// The lookup table has no notion of this node's own identifier, so it cannot verify that
    /// `candidate` actually belongs on the `direction` side of this node — callers must ensure
    /// that before calling. The comparison inside `try_link` only ever weighs the existing entry
    /// against `candidate`, never against this node itself, so a violated precondition installs
    /// an out-of-order neighbor silently rather than returning an error.
    ///
    /// # Errors
    ///
    /// returns an error when `level` is out of bounds.
    fn try_link(
        &self,
        level: LookupTableLevel,
        direction: Direction,
        candidate: Identity,
    ) -> anyhow::Result<LinkOutcome>;

    /// Atomically decides whether `claimant` should become — or already is — the neighbor at
    /// `(level, direction)`.
    ///
    /// This is the repair counterpart of [`Self::try_link`], and the two differ in what the
    /// caller is asking:
    ///
    /// - `try_link`'s `candidate` is a node requesting a link **for the first time** (e.g.
    ///   joining) — it is never already installed, so the only choices are to accept it or
    ///   forward the request onward.
    /// - `try_relink`'s `claimant` is a node **checking or re-asserting a link it believes it
    ///   should already hold**, run periodically by a background repair sweep to catch and fix
    ///   pointers that drifted out of sync — e.g. silently overwritten by a concurrent, otherwise
    ///   individually-correct `try_link` call landing on the same slot. Because the claimant may
    ///   already be correctly linked, there is a third possible outcome `try_link` has no use for.
    ///
    /// The decision is atomic and `direction` is receiver-owned, exactly as for `try_link` (see
    /// its docs for those general rules and for the "strictly between" comparison, which is
    /// identical here). Given the current entry at `(level, direction)`:
    ///
    /// - **it already equals `claimant`** — the pointer was never actually wrong: nothing is
    ///   written, [`RelinkOutcome::AlreadyConsistent`] is returned. This is what makes a repair
    ///   sweep over an already-healthy graph produce zero writes and zero further messages.
    /// - **it sits strictly between this node and `claimant`** — that neighbor is closer to
    ///   `claimant`'s true position, so it is not this node's call to make: the table is left
    ///   untouched and [`RelinkOutcome::Forward`] carries that neighbor, for the caller to retry
    ///   the check against.
    /// - **otherwise** (the slot is empty, or its occupant is neither `claimant` nor strictly
    ///   between this node and `claimant`) — `claimant` is installed as the new entry, and
    ///   [`RelinkOutcome::Relinked`] reports whatever was evicted (`None` if the slot was empty).
    ///   This method does not act on that eviction itself — it only reports it. The intent is
    ///   forward-looking: a future caller-side repair handler is expected to take the evicted
    ///   identity and issue a fresh check against *it*, so a fix can cascade outward and heal a
    ///   whole chain of stale pointers from a single triggering probe, not just the one slot
    ///   checked here. That handler does not yet exist in this codebase.
    ///
    /// # Preconditions
    ///
    /// Same as `try_link`: the lookup table has no notion of this node's own identifier, so
    /// callers must ensure `claimant` actually belongs on the `direction` side before calling —
    /// a violated precondition installs an out-of-order neighbor silently rather than erroring.
    ///
    /// # Errors
    ///
    /// returns an error when `level` is out of bounds.
    fn try_relink(
        &self,
        level: LookupTableLevel,
        direction: Direction,
        claimant: Identity,
    ) -> anyhow::Result<RelinkOutcome>;

    /// Returns the list of left neighbors at the current node as a vector of tuples containing
    /// the level and identity.
    ///
    /// Exercised only by tests today; retained because a future repair pass needs to enumerate
    /// every populated entry here, not just the highest, to probe each neighbor individually.
    fn left_neighbors(&self) -> Vec<(usize, Identity)>;

    /// Returns the list of right neighbors at the current node as a vector of tuples containing
    /// the level and identity. See [`Self::left_neighbors`] for why this is retained despite
    /// having no production caller today.
    fn right_neighbors(&self) -> Vec<(usize, Identity)>;

    /// Returns the highest level with a populated entry on either side.
    ///
    /// Unlike calling [`Self::left_neighbors`] and [`Self::right_neighbors`] separately and
    /// combining the results, the left and right sides are read atomically with respect to
    /// concurrent writes. No concurrent write can land between reading the two sides and leave
    /// the combined result reflecting no single consistent state of the table.
    ///
    /// # Returns
    ///
    /// `None` if no level has a populated entry on either side (an empty table). `Some(level)`
    /// for the highest level with a populated entry, otherwise.
    fn max_populated_level(&self) -> Option<LookupTableLevel>;

    /// Creates a shallow copy of this lookup table.
    ///
    /// Implementations should ensure that cloned instances share the same underlying data
    /// (e.g., using Arc for shared ownership). Changes made through one instance should be
    /// visible in all cloned instances. This is the standard cloning behavior for all
    /// LookupTable implementations.
    fn clone_box(&self) -> Box<dyn LookupTable>;
}

impl Clone for Box<dyn LookupTable> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}
