use crate::core::{IdSearchReq, IdSearchRes, Identifier, IrrevocableContext, MembershipVector};
use crate::network::Event::{IdSearchRequest, IdSearchResponse};
#[cfg(test)] // TODO: Remove once BaseNode is used in production code.
use crate::network::MessageProcessor;
use crate::network::{Event, EventProcessorCore, Network};
use crate::node::core::Core;
use anyhow::anyhow;
use std::fmt;
use std::fmt::Formatter;
use std::sync::mpsc::sync_channel;
use std::sync::{mpsc::SyncSender, Arc, Mutex};
use tracing::Span;

// TODO: Remove #[allow(dead_code)] once BaseNode is used in production code.
#[allow(dead_code)]
/// `BaseNode` is the network-aware orchestrator for a single skip-graph node.
///
/// It composes a `Box<dyn Core>` (the pure-local algorithms + lookup table)
/// with a `Box<dyn Network>` (the transport). All algorithmic work is
/// delegated to `core`; `BaseNode` is responsible only for wiring outbound
/// events, parking waiters for blocking originator calls, and routing
/// incoming events via `EventProcessorCore`.
pub(crate) struct BaseNode {
    core: Box<dyn Core>,
    net: Box<dyn Network>,
    span: Span,
    ctx: IrrevocableContext,
    ch: Arc<Mutex<Option<SyncSender<IdSearchRes>>>>,
}

impl BaseNode {
    /// Create a new `BaseNode` from an already-constructed `Core` and a
    /// network handle. Registers the node as an event processor on the
    /// network before returning.
    #[cfg(test)] // TODO: Remove once BaseNode is used in production code.
    pub(crate) fn new(
        parent_span: Span,
        core: Box<dyn Core>,
        net: Box<dyn Network>,
    ) -> anyhow::Result<Self> {
        let clone_net = net.clone();
        let span = tracing::span!(parent: &parent_span, tracing::Level::TRACE, "base_node", id = ?core.id(), mem_vec = ?core.mem_vec());
        let _enter = span.enter();

        let ctx = IrrevocableContext::new(&span, "base_node_context");

        let node = BaseNode {
            core,
            net,
            span: span.clone(),
            ctx,
            ch: Arc::new(Mutex::new(None)),
        };

        let processor = MessageProcessor::new(Box::new(node.clone()));

        if let Err(e) = clone_net.register_processor(processor) {
            let error = anyhow!("could not register node in network: {}", e);
            node.ctx.throw_irrecoverable(error);
        }

        tracing::trace!("successfully created and registered node");

        Ok(node)
    }

    /// Returns the node's identifier (delegated to core).
    #[allow(dead_code)]
    pub(crate) fn id(&self) -> &Identifier {
        self.core.id()
    }

    /// Returns the node's membership vector (delegated to core).
    #[allow(dead_code)]
    pub(crate) fn mem_vec(&self) -> &MembershipVector {
        self.core.mem_vec()
    }

    #[allow(dead_code)]
    pub(crate) fn search_by_id(&self, req: &IdSearchReq) -> anyhow::Result<IdSearchRes> {
        let span =
            tracing::trace_span!("search_by_id", target = ?req.target(), level = ?req.level());
        let _enter = span.enter();

        tracing::trace!("searching for target {:?}", req.target());
        let local_res = self
            .core
            .search_by_id(req)
            .map_err(|e| anyhow!("failed to perform search by id {}", e))?;
        if local_res.result() == self.core.id() {
            tracing::trace!("found self in search by id, terminating the search result");
            return Ok(local_res);
        }

        let (tx, rx) = sync_channel::<IdSearchRes>(1);
        {
            let mut slot = self.ch.lock().expect("mutex was poisoned by a previous panic");
            *slot = Some(tx);
        }
        let relay_request = IdSearchRequest(IdSearchReq::new(
            *self.core.id(),
            *req.target(),
            local_res.termination_level(),
            req.direction(),
        ));
        self.net
            .send_event(*local_res.result(), relay_request)
            .map_err(|e| anyhow!("failed to send relay request for search by id: {}", e))?;
        tracing::info!("relayed search by id request to the next node, pending response");
        let net_result = rx
            .recv()
            .map_err(|_| anyhow!("failed to receive response for search by id"))?;
        tracing::info!(
            "received network response for search by id {:?}: {:?}",
            *req.target(),
            net_result.result()
        );
        Ok(net_result)
    }
}

impl EventProcessorCore for BaseNode {
    fn process_incoming_event(&self, origin_id: Identifier, event: Event) -> anyhow::Result<()> {
        let _enter = self.span.enter();

        match event {
            IdSearchRequest(req) => {
                let span = tracing::trace_span!(
                    "search_by_id_request",
                    origin = ?origin_id,
                    target = ?req.target(),
                    direction = ?req.direction(),
                    level = ?req.level()
                );
                let _enter = span.enter();
                tracing::trace!("received request");

                let res = self
                    .core
                    .search_by_id(&req)
                    .map_err(|e| anyhow!("failed to perform search by id {}", e))?;

                let span = tracing::trace_span!(
                    "terminating",
                    result = ?res.result(),
                    termination_level = ?res.termination_level()
                );
                let _enter = span.enter();

                if res.result() == self.core.id() {
                    self.net
                        .send_event(*req.origin(), IdSearchResponse(res))
                        .map_err(|e| {
                            anyhow!("failed to send response event for search by id: {}", e)
                        })?;
                    tracing::info!("found self in search by id, terminated the search result");
                    return Ok(());
                }

                let relay_request = IdSearchRequest(crate::core::IdSearchReq::new(
                    *req.origin(),
                    *req.target(),
                    res.termination_level(),
                    req.direction(),
                ));
                self.net
                    .send_event(*res.result(), relay_request)
                    .map_err(|e| {
                        anyhow!(
                            "failed to send relay response event for search by id: {}",
                            e
                        )
                    })?;
                tracing::info!("relayed search by id request to the next node");
                Ok(())
            }
            IdSearchResponse(res) => {
                let span = tracing::trace_span!(
                    "search_by_id_response",
                    origin = ?origin_id,
                    target = ?res.target(),
                    result = ?res.result(),
                    termination_level = ?res.termination_level()
                );
                let _enter = span.enter();

                let waiter = self
                    .ch
                    .lock()
                    .expect("mutex was poisoned by a previous panic")
                    .take();
                if let Some(tx) = waiter {
                    if let Err(e) = tx.send(res) {
                        tracing::warn!("failed to send the response to the receiver end: {:?}", e)
                    }
                }

                Ok(())
            }
            _ => {
                tracing::warn!("received unsupported event payload type");
                Err(anyhow!("unsupported event payload type"))
            }
        }
    }
}

/// Two `BaseNode`s are equal if their core's id and membership vector match.
/// Network, context, and waiter slot are ignored.
impl PartialEq for BaseNode {
    fn eq(&self, other: &Self) -> bool {
        self.core.id() == other.core.id() && self.core.mem_vec() == other.core.mem_vec()
    }
}

impl fmt::Debug for BaseNode {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("BaseNode")
            .field("id", self.core.id())
            .field("mem_vec", self.core.mem_vec())
            .finish()
    }
}

impl Clone for BaseNode {
    fn clone(&self) -> Self {
        // Shallow clone: cloned instances share the same underlying core,
        // network, and waiter slot via Arc-backed boxes.
        BaseNode {
            core: self.core.clone(),
            net: self.net.clone(),
            span: self.span.clone(),
            ctx: self.ctx.clone(),
            ch: self.ch.clone(),
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
    use crate::network::NetworkMock;
    use crate::node::core::BaseCore;
    use unimock::*;

    #[test]
    fn test_base_node() {
        let id = random_identifier();
        let mem_vec = random_membership_vector();
        let span = span_fixture();

        let mock_net = Unimock::new((
            NetworkMock::register_processor
                .each_call(matching!(_))
                .answers(&|_, _| Ok(())),
            NetworkMock::clone_box
                .each_call(matching!())
                .answers(&|mock| Box::new(mock.clone())),
        ));

        let core = Box::new(BaseCore::new(
            span.clone(),
            id,
            mem_vec,
            Box::new(ArrayLookupTable::new()),
        ));

        let node = BaseNode::new(span.clone(), core, Box::new(mock_net)).unwrap();
        assert_eq!(node.id(), &id);
        assert_eq!(node.mem_vec(), &mem_vec);
    }
}
