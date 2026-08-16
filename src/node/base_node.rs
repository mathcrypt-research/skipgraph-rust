use crate::core::model::search::Nonce;
use crate::core::{
    IdSearchReq, IdSearchRes, Identifier, IrrevocableContext, LookupTableLevel, MaxLevelReq,
    MaxLevelRes, MembershipVector,
};
use crate::network::Event::{GetMaxLevelOp, RetMaxLevelOp, SearchByIdRequest, SearchByIdResponse};
#[cfg(test)] // TODO: Remove once BaseNode is used in production code.
use crate::network::MessageProcessor;
use crate::network::{Event, EventProcessorCore, Network};
use crate::node::core::Core;
use crate::node::waiter::{Waiter, WaiterGuard};
use anyhow::anyhow;
use std::collections::HashMap;
use std::fmt;
use std::fmt::Formatter;
use std::sync::mpsc::sync_channel;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::oneshot;
use tracing::{Instrument, Span};

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
    /// outstanding requests this node is waiting on, keyed by nonce; one map, one
    /// `Mutex`, for every message type (see [`Waiter`] for why).
    request_id_map: Arc<Mutex<HashMap<Nonce, Waiter>>>,
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
            request_id_map: Arc::new(Mutex::new(HashMap::new())),
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
    pub(crate) fn id(&self) -> Identifier {
        self.core.id()
    }

    /// Returns the node's membership vector (delegated to core).
    #[allow(dead_code)]
    pub(crate) fn mem_vec(&self) -> MembershipVector {
        self.core.mem_vec()
    }

    #[allow(dead_code)]
    pub(crate) fn search_by_id(&self, req: IdSearchReq) -> anyhow::Result<IdSearchRes> {
        let span = tracing::trace_span!("search_by_id", target = ?req.target, level = ?req.level);
        let _enter = span.enter();

        tracing::trace!("searching for target {:?}", req.target);
        let local_res = self
            .core
            .search_by_id(req)
            .map_err(|e| anyhow!("failed to perform search by id {}", e))?;
        if local_res.result == self.core.id() {
            tracing::trace!("found self in search by id, terminating the search result");
            return Ok(local_res);
        }

        let (tx, rx) = sync_channel::<IdSearchRes>(1);
        {
            let mut request_id_map = self
                .request_id_map
                .lock()
                .expect("mutex was poisoned by a previous panic");
            request_id_map.insert(req.nonce, Waiter::Search(tx));
        }
        let relay_request = SearchByIdRequest(IdSearchReq {
            nonce: req.nonce,
            target: req.target,
            origin: self.core.id(),
            level: local_res.termination_level,
            direction: req.direction,
        });

        if let Err(e) = self.net.send_event(local_res.result, relay_request) {
            self.request_id_map
                .lock()
                .expect("mutex was poisoned by a previous panic")
                .remove(&req.nonce);
            return Err(anyhow!("failed to perform search by id {}", e));
        }
        tracing::info!("relayed search by id request to the next node, pending response");
        match rx.recv() {
            Ok(net_result) => {
                tracing::info!(
                    "received network response for search by id {:?}: {:?}",
                    req.target,
                    net_result.result
                );
                Ok(net_result)
            }
            Err(_) => {
                self.request_id_map
                    .lock()
                    .expect("mutex was poisoned by a previous panic")
                    .remove(&req.nonce);
                Err(anyhow!(
                    "failed to receive network response for search by id"
                ))
            }
        }
    }

    /// Asks `introducer` for the highest lookup-table level at which it has any
    /// populated entry — phase 0 of the join bootstrap
    /// (`docs/protocol/concurrent-insert.md`, section 3.1); a latency optimization
    /// seeding the joining node's stage-1 search level, not a correctness requirement.
    /// Whichever way the call resolves, the waiter-map entry is cleaned up via
    /// [`WaiterGuard`] before returning.
    ///
    /// # Args
    ///
    /// * `introducer` — the node to query.
    /// * `timeout` — how long to wait for `introducer`'s reply before giving up.
    ///
    /// # Returns
    ///
    /// The highest lookup-table level at which `introducer` has a populated entry.
    ///
    /// # Errors
    ///
    /// * **RECOVERABLE** — sending the request to `introducer` fails. Since this call is
    ///   only a latency optimization, the caller may skip it and proceed without a
    ///   seeded level.
    /// * **RECOVERABLE** — the reply channel is dropped before a reply arrives.
    /// * **RECOVERABLE** — `timeout` elapses before a reply arrives.
    #[allow(dead_code)] // TODO: remove once phase-0 bootstrap is wired into join orchestration.
    pub(crate) async fn get_max_level(
        &self,
        introducer: Identifier,
        timeout: Duration,
    ) -> anyhow::Result<LookupTableLevel> {
        let span = tracing::trace_span!("get_max_level", introducer = ?introducer);

        // Attach the span via `.instrument()` rather than holding an `enter()` guard
        // across the `.await` below: the guard is `!Send` and would stay entered while
        // the future is suspended, leaking the span onto whatever unrelated work the
        // executor polls on this thread in the meantime.
        async move {
            let nonce = Nonce::random();
            let (tx, rx) = oneshot::channel::<MaxLevelRes>();

            {
                let mut request_id_map = self
                    .request_id_map
                    .lock()
                    .expect("mutex was poisoned by a previous panic");
                request_id_map.insert(nonce, Waiter::MaxLevel(tx));
            }
            // cleans up the map entry on every exit path, including cancellation. Never read
            // (its only job is running `Drop` at end of scope), hence the `_` prefix.
            let _guard = WaiterGuard::new(nonce, self.request_id_map.clone());

            if let Err(e) = self.net.send_event(
                introducer,
                GetMaxLevelOp(MaxLevelReq {
                    nonce,
                    origin: self.core.id(),
                }),
            ) {
                return Err(anyhow!("failed to send get max level request: {}", e));
            }
            tracing::info!("sent get max level request, pending response");

            match tokio::time::timeout(timeout, rx).await {
                Ok(Ok(res)) => {
                    tracing::info!("received max level response: {:?}", res.max_level);
                    Ok(res.max_level)
                }
                Ok(Err(_)) => Err(anyhow!(
                    "failed to receive network response for get max level: sender dropped"
                )),
                Err(_) => Err(anyhow!("timed out waiting for get max level response")),
            }
        }
        .instrument(span)
        .await
    }
}

impl EventProcessorCore for BaseNode {
    fn process_incoming_event(&self, origin_id: Identifier, event: Event) -> anyhow::Result<()> {
        let _enter = self.span.enter();

        match event {
            SearchByIdRequest(req) => {
                let span = tracing::trace_span!(
                    "search_by_id_request",
                    origin = ?origin_id,
                    target = ?req.target,
                    direction = ?req.direction,
                    level = ?req.level
                );
                let _enter = span.enter();
                tracing::trace!("received request");

                let res = self
                    .core
                    .search_by_id(req)
                    .map_err(|e| anyhow!("failed to perform search by id {}", e))?;

                let span = tracing::trace_span!(
                    "terminating",
                    result = ?res.result,
                    termination_level = ?res.termination_level
                );
                let _enter = span.enter();

                if res.result == self.core.id() {
                    self.net
                        .send_event(req.origin, SearchByIdResponse(res))
                        .map_err(|e| {
                            anyhow!("failed to send response event for search by id: {}", e)
                        })?;
                    tracing::info!("found self in search by id, terminated the search result");
                    return Ok(());
                }

                let relay_request = SearchByIdRequest(IdSearchReq {
                    level: res.termination_level,
                    ..req
                });

                self.net
                    .send_event(res.result, relay_request)
                    .map_err(|e| {
                        anyhow!(
                            "failed to send relay response event for search by id: {}",
                            e
                        )
                    })?;
                tracing::info!("relayed search by id request to the next node");
                Ok(())
            }
            SearchByIdResponse(res) => {
                let span = tracing::trace_span!(
                    "search_by_id_response",
                    origin = ?origin_id,
                    target = ?res.target,
                    result = ?res.result,
                    termination_level = ?res.termination_level
                );
                let _enter = span.enter();

                let waiter = self
                    .request_id_map
                    .lock()
                    .expect("mutex was poisoned by a previous panic")
                    .remove(&res.nonce);
                if let Some(Waiter::Search(tx)) = waiter {
                    if let Err(e) = tx.send(res) {
                        tracing::warn!("failed to send the response to the receiver end: {:?}", e)
                    }
                } else {
                    // no waiter, or an unexpected `Waiter::MaxLevel`: not this arm's
                    // concern, log and move on.
                    tracing::debug!("no matching search waiter for nonce {:?}", res.nonce);
                }

                Ok(())
            }
            RetMaxLevelOp(res) => {
                let waiter = self
                    .request_id_map
                    .lock()
                    .expect("mutex was poisoned by a previous panic")
                    .remove(&res.nonce);

                match waiter {
                    Some(Waiter::MaxLevel(tx)) => {
                        if let Err(e) = tx.send(res) {
                            tracing::warn!(
                                "failed to send the response to the receiver end: {:?}",
                                e
                            )
                        }
                    }
                    // no-op: an unknown/expired nonce or a wrong waiter kind is not an error.
                    _ => {
                        tracing::debug!("no matching max level waiter for nonce {:?}", res.nonce);
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
            .field("id", &self.core.id())
            .field("mem_vec", &self.core.mem_vec())
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
            request_id_map: self.request_id_map.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::direction::Direction;
    use crate::core::model::identity::Identity;
    use crate::core::testutil::fixtures::{
        random_address, random_identifier, random_identifier_greater_than,
        random_membership_vector, span_fixture,
    };
    use crate::core::{ArrayLookupTable, LookupTable};
    use crate::network::NetworkMock;
    use crate::node::core::BaseCore;
    use unimock::*;

    /// builds a `BaseNode` over `mock_net`, factoring out repeated core/node construction.
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
        assert_eq!(node.id(), id);
        assert_eq!(node.mem_vec(), mem_vec);
    }

    /// A single in-flight `get_max_level` call resolves to the level carried by its
    /// correlated `RetMaxLevelOp` reply.
    #[tokio::test]
    async fn test_get_max_level_resolves() {
        let id = random_identifier();
        let mem_vec = random_membership_vector();
        let span = span_fixture();
        let introducer = random_identifier();
        let expected_level: LookupTableLevel = 7;
        let nonce_cell: Arc<Mutex<Option<Nonce>>> = Arc::new(Mutex::new(None));
        let nonce_mock = nonce_cell.clone();

        let mock_net = Unimock::new((
            NetworkMock::register_processor
                .each_call(matching!(_))
                .answers(&|_, _| Ok(())),
            NetworkMock::clone_box
                .each_call(matching!())
                .answers(&|mock| Box::new(mock.clone())),
            NetworkMock::send_event
                .each_call(matching!(_))
                .answers_arc(Arc::new(move |_, dest: Identifier, event: Event| {
                    assert_eq!(dest, introducer, "expected request sent to the introducer");
                    match event {
                        GetMaxLevelOp(req) => {
                            *nonce_mock.lock().expect("mutex poisoned") = Some(req.nonce);
                            Ok(())
                        }
                        _ => panic!("unexpected event: {:?}", event),
                    }
                }))
                .once(),
        ));

        let core = Box::new(BaseCore::new(
            span.clone(),
            id,
            mem_vec,
            Box::new(ArrayLookupTable::new()),
        ));
        let node = BaseNode::new(span, core, Box::new(mock_net)).expect("failed to create node");
        let node_reply = node.clone();

        let (level_result, ()) = tokio::time::timeout(Duration::from_secs(2), async {
            tokio::join!(
                node.get_max_level(introducer, Duration::from_millis(200)),
                async {
                    // captured synchronously by the mock before get_max_level's first await.
                    let nonce = nonce_cell
                        .lock()
                        .expect("mutex poisoned")
                        .expect("nonce should already be captured");
                    node_reply
                        .process_incoming_event(
                            introducer,
                            RetMaxLevelOp(MaxLevelRes {
                                nonce,
                                max_level: expected_level,
                            }),
                        )
                        .expect("failed to process reply");
                }
            )
        })
        .await
        .expect("test timed out");

        assert_eq!(level_result.expect("should resolve"), expected_level);
    }

    /// Forces a blocking `search_by_id` waiter and an async `get_max_level` waiter to be
    /// live in the shared `request_id_map` simultaneously, then answers both. Guards three
    /// regressions.
    ///
    /// 1. The map's `Mutex` held across the blocking `recv` or across the `.await`, which
    ///    deadlocks the moment two waiters coexist.
    /// 2. Reply routing that resolves whichever waiter it finds instead of matching on the
    ///    nonce.
    /// 3. Eviction that ignores the `Waiter` variant and drops the sibling waiter.
    #[tokio::test]
    async fn test_concurrent_requests_of_different_types_resolve_independently() {
        let node_id = random_identifier();
        let mem_vec = random_membership_vector();
        let span = span_fixture();
        let introducer = random_identifier();

        // force the local search to resolve to a neighbor other than self, so
        // `search_by_id` takes the network-relay branch and registers a waiter.
        let lt = ArrayLookupTable::new();
        let target = random_identifier();
        let relay_target = random_identifier_greater_than(&target);
        lt.update_entry(
            Identity::new(relay_target, random_membership_vector(), random_address()),
            0,
            Direction::Left,
        )
        .expect("failed to update entry in lookup table");

        let expected_search_result = random_identifier();
        let expected_max_level: LookupTableLevel = 3;
        let search_nonce_cell: Arc<Mutex<Option<Nonce>>> = Arc::new(Mutex::new(None));
        let max_level_nonce_cell: Arc<Mutex<Option<Nonce>>> = Arc::new(Mutex::new(None));
        let (search_nonce_mock, max_level_nonce_mock) =
            (search_nonce_cell.clone(), max_level_nonce_cell.clone());

        let mock_net = Unimock::new((
            NetworkMock::register_processor
                .each_call(matching!(_))
                .answers(&|_, _| Ok(())),
            NetworkMock::clone_box
                .each_call(matching!())
                .answers(&|mock| Box::new(mock.clone())),
            NetworkMock::send_event
                .each_call(matching!(_))
                .answers_arc(Arc::new(
                    move |_, _: Identifier, event: Event| match event {
                        SearchByIdRequest(req) => {
                            *search_nonce_mock.lock().expect("mutex poisoned") = Some(req.nonce);
                            Ok(())
                        }
                        GetMaxLevelOp(req) => {
                            *max_level_nonce_mock.lock().expect("mutex poisoned") = Some(req.nonce);
                            Ok(())
                        }
                        _ => panic!("unexpected event: {:?}", event),
                    },
                )),
        ));

        let core = Box::new(BaseCore::new(span.clone(), node_id, mem_vec, Box::new(lt)));
        let node = BaseNode::new(span, core, Box::new(mock_net)).expect("failed to create node");

        let node_search = node.clone();
        let search_req = IdSearchReq {
            nonce: Nonce::random(),
            origin: node_id,
            target,
            level: 0,
            direction: Direction::Left,
        };
        let search_handle =
            tokio::task::spawn_blocking(move || node_search.search_by_id(search_req));
        // deliberately generous: this budget is spent waiting for the blocking search
        // thread to be scheduled, so a tight bound here fails under load. timeout
        // behaviour is covered by `test_get_max_level_times_out_and_cleans_up`, and the
        // outer bound below is what fails this test if anything hangs.
        let max_level_fut = node.get_max_level(introducer, Duration::from_secs(30));

        let deliver = async {
            // block until both requests are on the wire, in either order, so neither reply
            // can be delivered before its own waiter is registered.
            let (search_nonce, max_level_nonce) = loop {
                let s = *search_nonce_cell.lock().expect("mutex poisoned");
                let m = *max_level_nonce_cell.lock().expect("mutex poisoned");
                if let (Some(s), Some(m)) = (s, m) {
                    break (s, m);
                }
                tokio::task::yield_now().await;
            };
            node.process_incoming_event(
                introducer,
                RetMaxLevelOp(MaxLevelRes {
                    nonce: max_level_nonce,
                    max_level: expected_max_level,
                }),
            )
            .expect("failed to process max level reply");
            node.process_incoming_event(
                relay_target,
                SearchByIdResponse(IdSearchRes {
                    nonce: search_nonce,
                    target,
                    termination_level: 0,
                    result: expected_search_result,
                }),
            )
            .expect("failed to process search reply");
        };

        let (search_join_result, max_level_result, ()) =
            tokio::time::timeout(Duration::from_secs(2), async {
                tokio::join!(search_handle, max_level_fut, deliver)
            })
            .await
            .expect("test timed out");

        let search_result = search_join_result
            .expect("search_by_id task should not panic")
            .expect("search_by_id should resolve");
        assert_eq!(
            search_result.result, expected_search_result,
            "search_by_id must resolve to its own reply, not the max-level one"
        );
        assert_eq!(
            max_level_result.expect("get_max_level should resolve"),
            expected_max_level,
            "get_max_level must resolve to its own reply, not the search one"
        );
    }

    /// A `get_max_level` call with no reply delivered times out, and the waiter map no
    /// longer holds its entry afterward.
    #[tokio::test]
    async fn test_get_max_level_times_out_and_cleans_up() {
        let id = random_identifier();
        let mem_vec = random_membership_vector();
        let span = span_fixture();
        let introducer = random_identifier();

        let mock_net = Unimock::new((
            NetworkMock::register_processor
                .each_call(matching!(_))
                .answers(&|_, _| Ok(())),
            NetworkMock::clone_box
                .each_call(matching!())
                .answers(&|mock| Box::new(mock.clone())),
            NetworkMock::send_event
                .each_call(matching!(_))
                .answers(&|_, _, _| Ok(())),
        ));

        let core = Box::new(BaseCore::new(
            span.clone(),
            id,
            mem_vec,
            Box::new(ArrayLookupTable::new()),
        ));
        let node = BaseNode::new(span, core, Box::new(mock_net)).expect("failed to create node");

        let result = tokio::time::timeout(
            Duration::from_secs(1),
            node.get_max_level(introducer, Duration::from_millis(20)),
        )
        .await
        .expect("test itself should not time out");

        assert!(result.is_err(), "expected a timeout error");
        assert!(
            node.request_id_map
                .lock()
                .expect("mutex poisoned")
                .is_empty(),
            "expected the waiter map entry to be cleaned up"
        );
    }
}
