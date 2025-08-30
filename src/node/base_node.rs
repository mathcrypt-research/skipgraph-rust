use crate::core::model::direction::Direction;
use crate::core::{
    Identifier, IdSearchReq, IdSearchRes, LookupTable, MembershipVector,
};
#[cfg(test)] // TODO: Remove once BaseNode is used in production code.
use crate::network::MessageProcessor;
use crate::network::{EventProcessorCore, Network, Event};
use crate::node::Node;
use anyhow::anyhow;
use std::fmt;
use std::fmt::Formatter;
use tracing::Span;
use crate::network::Event::{IdSearchRequest, IdSearchResponse};

// TODO: Remove #[allow(dead_code)] once BaseNode is used in production code.
#[allow(dead_code)]
/// BaseNode is a struct that represents a single node in the implementation of the skip graph.
pub(crate) struct BaseNode {
    id: Identifier,
    mem_vec: MembershipVector,
    lt: Box<dyn LookupTable>,
    net: Box<dyn Network>,
    span: Span,
}

impl Node for BaseNode {
    fn get_identifier(&self) -> &Identifier {
        &self.id
    }

    fn get_membership_vector(&self) -> &MembershipVector {
        &self.mem_vec
    }

    /// Searches for an identifier in a level-based structure in a specific direction.
    ///
    /// This function attempts to find an identifier from the given `IdentifierSearchRequest`,
    /// by scanning through levels up to the specified level in the request and filtering values
    /// based on the direction. The result is either the best matching identifier or a fallback
    /// identifier if no matches are found.
    ///
    /// # Arguments
    /// - `req`: A reference to an [`IdSearchReq`] which contains the search criteria,
    ///   including the target identifier, the direction of the search (`Left` or `Right`), and
    ///   the level up to which the search should proceed.
    ///
    /// # Returns
    /// An [`anyhow::Result`] containing:
    /// - `Ok(IdentifierSearchResult)`: The result of the search, including:
    ///   - The target identifier (as given in the `req`),
    ///   - The level at which the closest match was found,
    ///   - The identifier of the closest match (or the current identifier if no close match was found).
    /// - `Err(anyhow::Error)`: An error if there was an issue while accessing a level or retrieving an entry.
    ///
    /// # Behavior
    /// - The function iterates through the levels from `0` to `req.level()`.
    /// - For each level, it retrieves an entry from the lookup table matching the
    ///   direction (`req.direction()`).
    /// - The entries collected are filtered:
    ///   - **Left direction**: Finds the smallest identifier greater than or equal to the target.
    ///   - **Right direction**: Finds the largest identifier less than or equal to the target.
    /// - If a valid match is found, the associated identifier and level are returned. Otherwise, it falls
    ///   back and returns its own identifier at level `0`.
    ///
    /// # Error Handling
    /// - If an error occurs while accessing an entry at any level, it immediately halts execution
    ///   and returns an `anyhow::Error`.
    ///
    /// # Notes
    /// - If no matching identifier is found at any level, the search defaults to returning
    ///   the caller's own identifier at level `0`. This edge behavior is covered in
    ///   `search_fallback_test.rs`.
    /// - The method aims to handle both leftward and rightward searches efficiently. To add support
    ///   for other directions or additional filtering, alterations may be required within the
    ///   filtering logic.
    ///
    /// # Returns Fields Explanation (via `IdentifierSearchResult`)
    /// - `target_id`: Copy of the target identifier for traceability purposes.
    /// - `level`: Indicates the level at which the match was found.
    /// - `matched_id`: The identifier of the closest match or fallback identifier.
    fn search_by_id(
        &self,
        req: &IdSearchReq,
    ) -> anyhow::Result<IdSearchRes> {
        let _enter = self.span.enter();
        tracing::trace!(
            "starting search_by_id for target {:?}, direction {:?}, max_level {}",
            req.target(),
            req.direction(),
            req.level()
        );

        // Collect neighbors from levels <= req.level in req.direction
        let candidates: Result<Vec<_>, _> = (0..=req.level())
            .filter_map(|lvl| match self.lt.get_entry(lvl, req.direction()) {
                Ok(Some(identity)) => Some(Ok((*identity.id(), lvl))),
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
            req.level()
        );

        // Filter candidates based on the direction
        let result = match req.direction() {
            Direction::Left => {
                // In the left direction, the result is the smallest identifier that is greater than or equal to the target
                candidates
                    .into_iter()
                    .filter(|(id, _)| id >= req.target())
                    .min_by_key(|(id, _)| *id)
            }
            Direction::Right => {
                // In the right direction, the result is the greatest identifier that is less than or equal to the target
                candidates
                    .into_iter()
                    .filter(|(id, _)| id <= req.target())
                    .max_by_key(|(id, _)| *id)
            }
        };

        match result {
            Some((id, level)) => {
                // If a candidate is found, return it
                let search_result = IdSearchRes::new(*req.target(), level, id);
                tracing::trace!(
                    "search successful: found match {:?} at level {}",
                    id,
                    level
                );
                Ok(search_result)
            }
            None => {
                // No valid neighbors were found at any level. As specified in
                // Aspnes & Shah's skip graph design, the search must fall back
                // to the caller's own identifier at level 0. See
                // `search_fallback_test.rs` for edge-case validation.
                tracing::trace!(
                    "search fallback: no valid candidates found, returning own identifier {:?}",
                    self.get_identifier()
                );
                Ok(IdSearchRes::new(
                    *req.target(),
                    0,
                    *self.get_identifier(),
                ))
            }
        }
    }

    fn search_by_mem_vec(
        &self,
        _req: &IdSearchReq,
    ) -> anyhow::Result<IdSearchRes> {
        todo!()
    }

    fn join(&self, _introducer: Identifier) -> anyhow::Result<()> {
        todo!()
    }
}

impl EventProcessorCore for BaseNode {
    fn process_incoming_event(&self, origin_id: Identifier, event: Event) -> anyhow::Result<()> {
        let _enter = self.span.enter();
        tracing::trace!("processing incoming event with target_node_id");

        match event {
            IdSearchRequest(req) => {
                tracing::trace!(
                    "received IdSearchRequest for target {:?}, direction {:?}, level {}",
                    req.target(),
                    req.direction(),
                    req.level()
                );

                let res = self.search_by_id(&req).map_err(|e| anyhow!("failed to perform search by id {}", e))?;
                let response_event = IdSearchResponse(res);

                tracing::trace!(
                    "sending IdSearchResponse with result {:?} at level {}",
                    res.result(),
                    res.termination_level()
                );

                // TODO: https://github.com/thep2p/skipgraph-rust/issues/43
                self.net.send_event(origin_id, response_event).map_err(|e| anyhow!("failed to send response event for search by id: {}", e))?;
                Ok(())
            }
            IdSearchResponse(res) => {
                // Handle the response (e.g., update state, notify waiting tasks, etc.)
                // For now, we just log it.
                tracing::trace!(
                    "received IdSearchResponse: target {:?}, result {:?}, level {}",
                    res.target(),
                    res.result(),
                    res.termination_level()
                );
                Ok(())
            }
            _ => {
                tracing::warn!("received unsupported event payload type");
                Err(anyhow!("unsupported event payload type"))
            }
        }
    }
}

impl BaseNode {
    /// Create a new `BaseNode` with the provided identifier, membership vector
    /// and lookup table.
    #[cfg(test)] // TODO: Remove once BaseNode is used in production code.
    pub(crate) fn new(
        parent_span: Span,
        id: Identifier,
        mem_vec: MembershipVector,
        lt: Box<dyn LookupTable>,
        net: Box<dyn Network>,
    ) -> anyhow::Result<Self> {
        let clone_net = net.clone();
        let span = tracing::span!(parent: parent_span, tracing::Level::TRACE, "base_node");
        let _enter = span.enter();
        
        tracing::trace!(
            "creating BaseNode with id {:?}, mem_vec {:?}",
            id,
            mem_vec
        );

        let node = BaseNode {
            id,
            mem_vec,
            lt,
            net,
            span: span.clone(),
        };

        // Create a MessageProcessor from this node, instead of casting directly
        let processor = MessageProcessor::new(Box::new(node.clone()));
        
        tracing::trace!(
            "registering BaseNode {:?} as event processor in network",
            id
        );

        clone_net
            .register_processor(processor)
            .map_err(|e| anyhow!("could not register node in network: {}", e))?;

        tracing::trace!(
            "successfully created and registered BaseNode {:?}",
            id
        );

        Ok(node)
    }
}

/// Implementing PartialEq for BaseNode to compare the id and membership vector.
/// This basically supports == operator for BaseNode.
/// The cardinal assumption is that the id and membership vector are unique for each node.
impl PartialEq for BaseNode {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && self.mem_vec == other.mem_vec
        // ignore lt for equality check as comparing trait objects is non-trivial
    }
}

impl fmt::Debug for BaseNode {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("BaseNode")
            .field("id", &self.id)
            .field("mem_vec", &self.mem_vec)
            .finish()
    }
}

impl Clone for BaseNode {
    fn clone(&self) -> Self {
        BaseNode {
            id: self.id,
            mem_vec: self.mem_vec,
            lt: self.lt.clone(),
            net: self.net.clone(),
            span: self.span.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::testutil::fixtures::{
        random_identifier, random_membership_vector, span_fixture,
    };
    use crate::core::ArrayLookupTable;
    use unimock::*;

    #[test]
    fn test_base_node() {
        let id = random_identifier();
        let mem_vec = random_membership_vector();
        let node = BaseNode {
            id,
            mem_vec,
            lt: Box::new(ArrayLookupTable::new(&span_fixture())),
            net: Box::new(Unimock::new(())), // No expectations needed for direct struct construction
            span: span_fixture(),
        };
        assert_eq!(node.get_identifier(), &id);
        assert_eq!(node.get_membership_vector(), &mem_vec);
    }
}
