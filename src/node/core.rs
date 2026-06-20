use crate::core::model::direction::Direction;
use crate::core::{IdSearchReq, IdSearchRes, Identifier, LookupTable, MembershipVector};
use anyhow::anyhow;
use tracing::Span;

/// Core is the pure-local interface for a skip-graph node's algorithms.
///
/// All methods follow a message-in / message-out shape: callers pass a
/// request type and receive a result type. Stateless operations (search) are
/// deterministic functions of the underlying lookup table; future stateful
/// operations (join, delete) will maintain protocol state internally across
/// calls.
///
/// `Core` knows nothing about the network. Network orchestration lives in
/// `BaseNode`, which owns a `Box<dyn Core>` and routes events to/from it.
pub trait Core: Send + Sync {
    /// Returns the identifier of the node this core belongs to.
    fn id(&self) -> Identifier;

    /// Returns the membership vector of the node this core belongs to.
    fn mem_vec(&self) -> MembershipVector;

    /// Performs a local search for the given identifier in the lookup table
    /// in the direction and up to the level specified by the request. The
    /// result is the closest neighbor satisfying the directional constraint,
    /// or — if no such neighbor exists at any level — the caller's own
    /// identifier at level 0 (the Aspnes & Shah fallback).
    fn search_by_id(&self, req: IdSearchReq) -> anyhow::Result<IdSearchRes>;

    /// Performs a local search for the given membership vector.
    #[allow(dead_code)]
    fn search_by_mem_vec(&self, req: IdSearchReq) -> anyhow::Result<IdSearchRes>;

    /// Shallow-clones this core. Cloned instances share the same underlying
    /// state (lookup table, etc.) via Arc.
    fn clone_box(&self) -> Box<dyn Core>;
}

impl Clone for Box<dyn Core> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

/// `BaseCore` is the concrete `Core` implementation backed by an
/// `ArrayLookupTable`-style lookup table. It owns the node's identifier,
/// membership vector, and lookup table. All state is shallow-cloneable via
/// the Arc-backed lookup table; cloned instances share the same LT.
// TODO: Remove #[allow(dead_code)] once BaseCore is used in production code.
#[allow(dead_code)]
pub struct BaseCore {
    id: Identifier,
    mem_vec: MembershipVector,
    lt: Box<dyn LookupTable>,
    span: Span,
}

impl BaseCore {
    #[cfg(test)] // TODO: remove once BaseCore is used in production code.
    pub(crate) fn new(
        parent_span: Span,
        id: Identifier,
        mem_vec: MembershipVector,
        lt: Box<dyn LookupTable>,
    ) -> Self {
        let span = tracing::span!(parent: &parent_span, tracing::Level::TRACE, "base_core", id = ?id, mem_vec = ?mem_vec);
        BaseCore {
            id,
            mem_vec,
            lt,
            span,
        }
    }
}

impl Clone for BaseCore {
    fn clone(&self) -> Self {
        // Shallow clone: cloned instances share the same underlying lookup
        // table via Arc. Any new fields added here must maintain this contract.
        BaseCore {
            id: self.id,
            mem_vec: self.mem_vec,
            lt: self.lt.clone(),
            span: self.span.clone(),
        }
    }
}

impl Core for BaseCore {
    fn id(&self) -> Identifier {
        self.id
    }

    fn mem_vec(&self) -> MembershipVector {
        self.mem_vec
    }

    fn search_by_id(&self, req: IdSearchReq) -> anyhow::Result<IdSearchRes> {
        let span = tracing::trace_span!(
            parent: &self.span,
            "search_by_id_req",
            target = ?req.target,
            dir = ?req.direction,
            level = ?req.level
        );
        let _enter = span.enter();

        // Collect neighbors from levels <= req.level in req.direction
        let candidates: Result<Vec<_>, _> = (0..=req.level)
            .filter_map(|lvl| match self.lt.get_entry(lvl, req.direction) {
                Ok(Some(identity)) => Some(Ok((identity.id(), lvl))),
                Ok(None) => None,
                Err(e) => Some(Err(anyhow!(
                    "error while searching by id in level {}: {}",
                    lvl,
                    e
                ))),
            })
            .collect();

        let candidates = candidates?;

        tracing::trace!(
            "found {} candidates across levels 0-{}",
            candidates.len(),
            req.level
        );

        // Filter candidates based on the direction
        let result = match req.direction {
            Direction::Left => {
                // smallest identifier that is >= target
                candidates
                    .into_iter()
                    .filter(|(id, _)| id >= &req.target)
                    .min_by_key(|(id, _)| *id)
            }
            Direction::Right => {
                // greatest identifier that is <= target
                candidates
                    .into_iter()
                    .filter(|(id, _)| id <= &req.target)
                    .max_by_key(|(id, _)| *id)
            }
        };

        match result {
            Some((id, level)) => {
                let search_result = IdSearchRes {
                    nonce: req.nonce,
                    target: req.target,
                    termination_level: level,
                    result: id,
                };
                tracing::trace!("search successful: found match {:?} at level {}", id, level);
                Ok(search_result)
            }
            None => {
                // No valid neighbors at any level: Aspnes & Shah fallback —
                // return caller's own identifier at level 0.
                tracing::trace!(
                    "search fallback: no valid candidates found, returning own identifier {:?}",
                    self.id
                );
                Ok(IdSearchRes {
                    nonce: req.nonce,
                    target: req.target,
                    termination_level: 0,
                    result: self.id,
                })
            }
        }
    }

    fn search_by_mem_vec(&self, _req: IdSearchReq) -> anyhow::Result<IdSearchRes> {
        todo!()
    }

    fn clone_box(&self) -> Box<dyn Core> {
        Box::new(self.clone())
    }
}
