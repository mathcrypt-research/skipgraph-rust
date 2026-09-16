use crate::core::model::search::Nonce;
use crate::core::{
    Direction, IdSearchReq, IdSearchRes, Identifier, Identity, IrrevocableContext, LinkReq,
    LinkRes, LookupTableLevel, MaxLevelReq, MaxLevelRes, MembershipVector, NeighborReq,
    NeighborRes, LOOKUP_TABLE_LEVELS,
};
use crate::network::Event::{
    GetLinkOp, GetMaxLevelOp, GetNeighborOp, RetMaxLevelOp, RetNeighborOp, SearchByIdRequest,
    SearchByIdResponse, SetLinkOp,
};
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
use tracing::Span;

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
        let span = tracing::span!(parent: &parent_span, tracing::Level::DEBUG, "base_node", id = ?core.id(), mem_vec = ?core.mem_vec());
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
    pub(crate) fn id(&self) -> Identifier {
        self.core.id()
    }

    /// Returns the node's membership vector (delegated to core).
    pub(crate) fn mem_vec(&self) -> MembershipVector {
        self.core.mem_vec()
    }

    pub(crate) fn search_by_id(&self, req: IdSearchReq) -> anyhow::Result<IdSearchRes> {
        let span = tracing::trace_span!(parent: &self.span, "search_by_id", target = ?req.target, level = ?req.level);
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
    #[tracing::instrument(level = "trace", parent = &self.span, fields(introducer = ?introducer), skip(self, timeout))]
    pub(crate) async fn get_max_level(
        &self,
        introducer: Identifier,
        timeout: Duration,
    ) -> anyhow::Result<LookupTableLevel> {
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

    /// Sends `SearchByIdRequest` directly to `introducer`, bypassing this node's own
    /// local search entirely. Stage 1's search must not run `Core::search_by_id`
    /// against this node's own (still empty, pre-join) table first, since that
    /// method's local-first fallback would immediately return this node's own
    /// identifier and short-circuit before ever contacting `introducer`. Resolved by
    /// the existing `SearchByIdResponse` handling in `process_incoming_event`, which
    /// also checks for a [`Waiter::AsyncSearch`] alongside its blocking-caller
    /// `Waiter::Search` counterpart.
    ///
    /// `Core::search_by_id`'s relay chain preserves, at every hop after the first, the
    /// invariant that the local candidate search is only ever asked of a node already
    /// known to satisfy `direction`'s relation to `target` (that's how each hop got
    /// selected as the previous hop's candidate), so its self-fallback ("no better
    /// candidate, terminate here") is only sound once that invariant already holds at
    /// the very first hop. The caller is responsible for choosing `direction` such
    /// that `introducer` itself already satisfies it relative to this node's own id.
    ///
    /// # Args
    ///
    /// * `introducer`, the node to search from.
    /// * `direction`, the search direction; the caller must ensure `introducer`'s own
    ///   id already satisfies this direction's relation to this node's own id.
    /// * `level`, the starting lookup-table level for the search, typically seeded
    ///   by [`Self::get_max_level`].
    /// * `timeout`, how long to wait for the terminal node's reply before giving up.
    ///
    /// # Returns
    ///
    /// The identifier of the search's terminal node.
    ///
    /// # Errors
    ///
    /// * **RECOVERABLE, INTERNAL.** Sending the request fails, the reply channel is
    ///   dropped, or `timeout` elapses before a reply arrives.
    #[tracing::instrument(
        parent = &self.span,
        fields(introducer = ?introducer, direction = ?direction, level = ?level),
        skip(self, timeout)
    )]
    async fn search_stage1_anchor(
        &self,
        introducer: Identifier,
        direction: Direction,
        level: LookupTableLevel,
        timeout: Duration,
    ) -> anyhow::Result<Identifier> {
        let nonce = Nonce::random();
        let (tx, rx) = oneshot::channel::<IdSearchRes>();

        {
            let mut request_id_map = self
                .request_id_map
                .lock()
                .expect("mutex was poisoned by a previous panic");
            request_id_map.insert(nonce, Waiter::AsyncSearch(tx));
        }
        let _guard = WaiterGuard::new(nonce, self.request_id_map.clone());

        self.net
            .send_event(
                introducer,
                SearchByIdRequest(IdSearchReq {
                    nonce,
                    target: self.core.id(),
                    origin: self.core.id(),
                    level,
                    direction,
                }),
            )
            .map_err(|e| anyhow!("failed to send stage-1 search request: {}", e))?;
        tracing::info!("sent stage-1 search request, pending response");

        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(res)) => Ok(res.result),
            Ok(Err(_)) => Err(anyhow!(
                "failed to receive stage-1 search response: sender dropped"
            )),
            Err(_) => Err(anyhow!("timed out waiting for stage-1 search response")),
        }
    }

    /// Asks `from` for its current neighbor entry on `direction` at level 0, part of
    /// stage 1 of the join protocol.
    ///
    /// # Args
    ///
    /// * `from`, the node to query.
    /// * `direction`, which of `from`'s own slots to query.
    /// * `timeout`, how long to wait for `from`'s reply before giving up.
    ///
    /// # Returns
    ///
    /// `from`'s current neighbor on `direction` at level 0, or `None` if `from`
    /// currently believes it has none there.
    ///
    /// # Errors
    ///
    /// * **RECOVERABLE, INTERNAL.** Sending the request fails, the reply channel is
    ///   dropped, or `timeout` elapses before a reply arrives.
    #[tracing::instrument(
        parent = &self.span,
        fields(from = ?from, direction = ?direction),
        skip(self, timeout)
    )]
    async fn get_neighbor(
        &self,
        from: Identifier,
        direction: Direction,
        timeout: Duration,
    ) -> anyhow::Result<Option<Identity>> {
        let nonce = Nonce::random();
        let (tx, rx) = oneshot::channel::<NeighborRes>();

        {
            let mut request_id_map = self
                .request_id_map
                .lock()
                .expect("mutex was poisoned by a previous panic");
            request_id_map.insert(nonce, Waiter::Neighbor(tx));
        }
        let _guard = WaiterGuard::new(nonce, self.request_id_map.clone());

        self.net
            .send_event(
                from,
                GetNeighborOp(NeighborReq {
                    nonce,
                    origin: self.core.id(),
                    level: 0,
                    direction,
                }),
            )
            .map_err(|e| anyhow!("failed to send get neighbor request: {}", e))?;
        tracing::info!("sent get neighbor request, pending response");

        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(res)) => Ok(res.neighbor),
            Ok(Err(_)) => Err(anyhow!(
                "failed to receive get neighbor response: sender dropped"
            )),
            Err(_) => Err(anyhow!("timed out waiting for get neighbor response")),
        }
    }

    /// Sends `GetLinkOp` to `dest`, asking it to adopt `candidate` as its neighbor
    /// on `side` at `level`, part of stage 1 of the join protocol.
    ///
    /// The resulting `SetLinkOp` reply is applied to this node's own lookup table by
    /// the existing handling in `process_incoming_event`, not by this method. By the
    /// time this method's `await` resolves, that write has already happened, since
    /// both run on the same synchronous event-processing path.
    ///
    /// # Args
    ///
    /// * `dest`, the node to send the link request to.
    /// * `candidate`, this node's own identity, proposed as `dest`'s neighbor.
    /// * `side`, receiver-owned, which of `dest`'s own slots `candidate` is
    ///   proposed for.
    /// * `level`, the lookup-table level at which the link is requested.
    /// * `timeout`, how long to wait for a reply before giving up.
    ///
    /// # Errors
    ///
    /// * **RECOVERABLE, INTERNAL.** Sending the request fails, the reply channel is
    ///   dropped, or `timeout` elapses before a reply arrives.
    #[tracing::instrument(
        parent = &self.span,
        fields(dest = ?dest, side = ?side, level = ?level),
        skip(self, candidate, timeout)
    )]
    async fn send_link_request(
        &self,
        dest: Identifier,
        candidate: Identity,
        side: Direction,
        level: LookupTableLevel,
        timeout: Duration,
    ) -> anyhow::Result<()> {
        let nonce = Nonce::random();
        let (tx, rx) = oneshot::channel::<LinkRes>();

        {
            let mut request_id_map = self
                .request_id_map
                .lock()
                .expect("mutex was poisoned by a previous panic");
            request_id_map.insert(nonce, Waiter::Link(tx));
        }
        let _guard = WaiterGuard::new(nonce, self.request_id_map.clone());

        self.net
            .send_event(
                dest,
                GetLinkOp(LinkReq {
                    nonce,
                    candidate,
                    side,
                    level,
                }),
            )
            .map_err(|e| anyhow!("failed to send get link request: {}", e))?;
        tracing::info!("sent get link request, pending response");

        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(res)) => {
                tracing::info!("resolved link request: linked = {:?}", res.linked);
                Ok(())
            }
            Ok(Err(_)) => Err(anyhow!(
                "failed to receive get link response: sender dropped"
            )),
            Err(_) => Err(anyhow!("timed out waiting for get link response")),
        }
    }

    /// Drives stage 1, level-0 linking, of the join protocol for this not-yet-joined
    /// node.
    ///
    /// `introducer` is an arbitrary, externally-supplied node with no guaranteed
    /// position relative to this node's own id, so the search direction cannot be
    /// fixed: this node picks `Direction::Right` when `introducer`'s id is less than
    /// its own (searching for its predecessor `s`, mirroring today's single-branch
    /// behavior) and `Direction::Left` when `introducer`'s id is greater (searching
    /// for its successor `z` instead), in both cases so `introducer` itself already
    /// satisfies the chosen direction's relation to this node's own id, which is what
    /// makes `Core::search_by_id`'s relay chain (and its self-terminating fallback)
    /// sound starting from the very first hop. `introducer`'s id equal to this node's
    /// own is an id collision, not something either branch resolves.
    ///
    /// Whichever node the search resolves is queried, on the same direction, for its
    /// own neighbor there; that query's answer, if any, sits on the opposite side.
    /// This reproduces `s` and `z` exactly as the two-branch structure above intends:
    /// searching right resolves `s` directly and queries `s`'s own right neighbor for
    /// `z`; searching left resolves `z` directly and queries `z`'s own left neighbor
    /// for `s`. The node the search resolved always gets offered the search's own
    /// direction as its `GetLinkOp` side (`s` is always offered `Direction::Right`,
    /// `z` always `Direction::Left`), and the neighbor-query result, when present, is
    /// always offered the opposite side. The two requests are sent concurrently and
    /// each resolves a structurally different side of this node's own table, so
    /// neither is redundant with the other.
    ///
    /// When the neighbor query resolves to `None`, no request is ever sent for that
    /// side. That is expected, not an error, this node's corresponding side (its
    /// right side when searching right, its left side when searching left, i.e. when
    /// this node is becoming the new largest or smallest node reachable from
    /// `introducer`, respectively) is simply left unresolved for now, to be healed
    /// later by background repair, which is out of this method's scope. The search's
    /// own result always resolves, since the graph is non-empty by construction.
    /// Every `SetLinkOp` reply this method waits on is applied to this node's own
    /// table by `process_incoming_event`, not by this method.
    ///
    /// # Args
    ///
    /// * `introducer`, an already-joined node used to seed the stage-1 search.
    /// * `max_level`, the starting lookup-table level for the stage-1 search,
    ///   typically `introducer`'s own reply to [`Self::get_max_level`].
    /// * `timeout`, the bound applied to every individual round trip this method
    ///   performs.
    ///
    /// # Errors
    ///
    /// * **RECOVERABLE, INTERNAL.** `introducer`'s id equals this node's own id.
    /// * **RECOVERABLE, INTERNAL.** Any of the underlying round trips fails to send,
    ///   has its reply channel dropped, or times out.
    #[tracing::instrument(
        parent = &self.span,
        fields(introducer = ?introducer, max_level = ?max_level),
        skip(self, timeout)
    )]
    pub(crate) async fn join_stage1_link_level0(
        &self,
        introducer: Identifier,
        max_level: LookupTableLevel,
        timeout: Duration,
    ) -> anyhow::Result<()> {
        let own_id = self.core.id();
        let search_direction = if introducer < own_id {
            Direction::Right
        } else if introducer > own_id {
            Direction::Left
        } else {
            return Err(anyhow!(
                "introducer id collides with this node's own id, cannot determine stage-1 search direction"
            ));
        };
        let query_direction = match search_direction {
            Direction::Right => Direction::Left,
            Direction::Left => Direction::Right,
        };

        let search_result = self
            .search_stage1_anchor(introducer, search_direction, max_level, timeout)
            .await?;
        tracing::info!(
            "resolved stage-1 search anchor {:?} on {:?}",
            search_result,
            search_direction
        );

        let query_result = self
            .get_neighbor(search_result, search_direction, timeout)
            .await?;

        let u_identity = Identity::new(own_id, self.core.mem_vec(), self.net.address());

        let search_link =
            self.send_link_request(search_result, u_identity, search_direction, 0, timeout);
        match query_result {
            Some(query_identity) => {
                tracing::info!(
                    "resolved stage-1 neighbor query result {:?} on {:?}",
                    query_identity,
                    query_direction
                );
                let query_link = self.send_link_request(
                    query_identity.id(),
                    u_identity,
                    query_direction,
                    0,
                    timeout,
                );
                let (search_res, query_res) = tokio::join!(search_link, query_link);
                search_res?;
                query_res?;
            }
            None => {
                tracing::info!(
                    "no neighbor at query time on {:?}, that side left unresolved",
                    query_direction
                );
                search_link.await?;
            }
        }

        tracing::info!("stage-1 (level-0) join linking complete");
        Ok(())
    }
}

impl EventProcessorCore for BaseNode {
    fn process_incoming_event(&self, origin_id: Identifier, event: Event) -> anyhow::Result<()> {
        let _enter = self.span.enter();

        match event {
            SearchByIdRequest(req) => {
                let request_span = tracing::trace_span!(
                    parent: &self.span,
                    "search_by_id_request",
                    origin = ?origin_id,
                    target = ?req.target,
                    direction = ?req.direction,
                    level = ?req.level
                );
                let _request_enter = request_span.enter();
                tracing::trace!("received request");

                let res = self
                    .core
                    .search_by_id(req)
                    .map_err(|e| anyhow!("failed to perform search by id {}", e))?;

                let span = tracing::trace_span!(
                    parent: &request_span,
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
                    parent: &self.span,
                    "search_by_id_response",
                    origin = ?origin_id,
                    target = ?res.target,
                    result = ?res.result,
                    termination_level = ?res.termination_level
                );
                let _enter = span.enter();

                let mut request_id_map = self
                    .request_id_map
                    .lock()
                    .expect("mutex was poisoned by a previous panic");
                let waiter = match request_id_map.get(&res.nonce) {
                    Some(Waiter::Search(_)) | Some(Waiter::AsyncSearch(_)) => {
                        request_id_map.remove(&res.nonce)
                    }
                    _ => None,
                };
                drop(request_id_map);

                match waiter {
                    Some(Waiter::Search(tx)) => {
                        if let Err(e) = tx.send(res) {
                            tracing::warn!(
                                "failed to send the response to the receiver end: {:?}",
                                e
                            )
                        }
                    }
                    Some(Waiter::AsyncSearch(tx)) => {
                        if tx.send(res).is_err() {
                            tracing::warn!(
                                "failed to send the async search response to the receiver end"
                            )
                        }
                    }
                    _ => {
                        // no waiter at this nonce, or the entry belongs to an unrelated
                        // waiter variant: left untouched in the map, not this arm's
                        // concern. log and move on.
                        tracing::debug!("no matching search waiter for nonce {:?}", res.nonce);
                    }
                }

                Ok(())
            }
            RetMaxLevelOp(res) => {
                let mut request_id_map = self
                    .request_id_map
                    .lock()
                    .expect("mutex was poisoned by a previous panic");
                let waiter = if matches!(request_id_map.get(&res.nonce), Some(Waiter::MaxLevel(_)))
                {
                    request_id_map.remove(&res.nonce)
                } else {
                    None
                };
                drop(request_id_map);

                if let Some(Waiter::MaxLevel(tx)) = waiter {
                    if let Err(e) = tx.send(res) {
                        tracing::warn!("failed to send the response to the receiver end: {:?}", e)
                    }
                } else {
                    // no waiter at this nonce, or the entry belongs to an unrelated
                    // waiter variant: left untouched in the map, not this arm's
                    // concern. log and move on.
                    tracing::debug!("no matching max level waiter for nonce {:?}", res.nonce);
                }

                Ok(())
            }
            RetNeighborOp(res) => {
                let span = tracing::trace_span!(
                    parent: &self.span,
                    "ret_neighbor_op",
                    origin = ?origin_id,
                    level = ?res.level,
                    direction = ?res.direction,
                    neighbor = ?res.neighbor
                );
                let _enter = span.enter();

                let mut request_id_map = self
                    .request_id_map
                    .lock()
                    .expect("mutex was poisoned by a previous panic");
                let waiter = if matches!(request_id_map.get(&res.nonce), Some(Waiter::Neighbor(_)))
                {
                    request_id_map.remove(&res.nonce)
                } else {
                    None
                };
                drop(request_id_map);

                if let Some(Waiter::Neighbor(tx)) = waiter {
                    if let Err(e) = tx.send(res) {
                        tracing::warn!("failed to send the response to the receiver end: {:?}", e)
                    }
                } else {
                    // no waiter at this nonce, or the entry belongs to an unrelated
                    // waiter variant: left untouched in the map, not this arm's
                    // concern. log and move on.
                    tracing::debug!("no matching neighbor waiter for nonce {:?}", res.nonce);
                }

                Ok(())
            }
            SetLinkOp(res) => {
                let span = tracing::trace_span!(
                    parent: &self.span,
                    "set_link_op",
                    origin = ?origin_id,
                    side = ?res.side,
                    level = ?res.level,
                    linked = ?res.linked
                );
                let _enter = span.enter();

                // this is the one and only write path for a lookup-table entry from a
                // `SetLinkOp`. it is attempted unconditionally whenever `linked` is
                // present and `level` is in range, independent of whether a matching
                // waiter exists below. `try_link` itself may still leave the table
                // untouched (its `LinkOutcome::Forward` case, when the existing entry
                // already sits correctly), so "attempted" here does not guarantee a
                // write occurred. `SetLinkOp` can also arrive unsolicited as a future
                // repair-push correction, which must be attempted the same way
                // regardless of any pending request.
                //
                // `res.level` is peer-controlled and, per the previous paragraph, has
                // no accompanying local request to sanity-check it against — unlike
                // `Core::try_link`'s own doc, which classifies every failure as this
                // node's own broken invariant (CRITICAL, INTERNAL) on the assumption
                // that `level` is already known-good by the time it's called. At this
                // boundary that assumption doesn't hold, so an out-of-range `level` is
                // classified RECOVERABLE, PEER-SAFE-detectable instead: it means a
                // malformed or adversarial peer message, not a local invariant
                // violation, so it's logged and the write is skipped rather than
                // propagated as a hard error from this arm. A `try_link` failure at an
                // in-range level (e.g. a poisoned local lock) is still that genuine
                // CRITICAL, INTERNAL case and propagates as before.
                if let Some(linked) = res.linked {
                    if res.level >= LOOKUP_TABLE_LEVELS {
                        tracing::warn!(
                            "rejected set link op from peer with out-of-range level {}",
                            res.level
                        );
                    } else {
                        self.core
                            .try_link(res.level, res.side, linked)
                            .map_err(|e| {
                                anyhow!("failed to apply link at level {}: {}", res.level, e)
                            })?;
                        tracing::trace!("applied link to own lookup table");
                    }
                }

                let mut request_id_map = self
                    .request_id_map
                    .lock()
                    .expect("mutex was poisoned by a previous panic");
                let waiter = if matches!(request_id_map.get(&res.nonce), Some(Waiter::Link(_))) {
                    request_id_map.remove(&res.nonce)
                } else {
                    None
                };
                drop(request_id_map);

                if let Some(Waiter::Link(tx)) = waiter {
                    if let Err(e) = tx.send(res) {
                        tracing::warn!("failed to send the response to the receiver end: {:?}", e)
                    }
                } else {
                    // no waiter at this nonce, or the entry belongs to an unrelated
                    // waiter variant: left untouched in the map, not this arm's
                    // concern. log and move on.
                    tracing::debug!("no matching link waiter for nonce {:?}", res.nonce);
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
        random_identifier_less_than, random_identity, random_membership_vector, span_fixture,
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

    /// A single in-flight `search_stage1_anchor` call resolves to the identifier
    /// carried by its correlated `SearchByIdResponse` reply, and the outbound request
    /// carries the caller-supplied `direction`.
    #[tokio::test]
    async fn test_search_stage1_anchor_resolves() {
        let id = random_identifier();
        let mem_vec = random_membership_vector();
        let span = span_fixture();
        let introducer = random_identifier();
        let expected_anchor = random_identifier();
        let search_level: LookupTableLevel = 3;
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
                        SearchByIdRequest(req) => {
                            assert_eq!(req.direction, Direction::Right);
                            assert_eq!(req.level, search_level);
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

        let (anchor_result, ()) = tokio::time::timeout(Duration::from_secs(2), async {
            tokio::join!(
                node.search_stage1_anchor(
                    introducer,
                    Direction::Right,
                    search_level,
                    Duration::from_millis(200)
                ),
                async {
                    // captured synchronously by the mock before search_stage1_anchor's
                    // first await.
                    let nonce = nonce_cell
                        .lock()
                        .expect("mutex poisoned")
                        .expect("nonce should already be captured");
                    node_reply
                        .process_incoming_event(
                            introducer,
                            SearchByIdResponse(IdSearchRes {
                                nonce,
                                target: id,
                                termination_level: search_level,
                                result: expected_anchor,
                            }),
                        )
                        .expect("failed to process reply");
                }
            )
        })
        .await
        .expect("test timed out");

        assert_eq!(anchor_result.expect("should resolve"), expected_anchor);
    }

    /// A single in-flight `get_neighbor` call resolves to the neighbor entry carried
    /// by its correlated `RetNeighborOp` reply, and the outbound request carries the
    /// caller-supplied `direction`.
    #[tokio::test]
    async fn test_get_neighbor_resolves() {
        let id = random_identifier();
        let mem_vec = random_membership_vector();
        let span = span_fixture();
        let from = random_identifier();
        let expected_neighbor = random_identity();
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
                    assert_eq!(dest, from, "expected request sent to the queried node");
                    match event {
                        GetNeighborOp(req) => {
                            assert_eq!(req.direction, Direction::Right);
                            assert_eq!(req.level, 0);
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

        let (neighbor_result, ()) = tokio::time::timeout(Duration::from_secs(2), async {
            tokio::join!(
                node.get_neighbor(from, Direction::Right, Duration::from_millis(200)),
                async {
                    // captured synchronously by the mock before get_neighbor's first
                    // await.
                    let nonce = nonce_cell
                        .lock()
                        .expect("mutex poisoned")
                        .expect("nonce should already be captured");
                    node_reply
                        .process_incoming_event(
                            from,
                            RetNeighborOp(NeighborRes {
                                nonce,
                                level: 0,
                                direction: Direction::Right,
                                neighbor: Some(expected_neighbor),
                            }),
                        )
                        .expect("failed to process reply");
                }
            )
        })
        .await
        .expect("test timed out");

        assert_eq!(
            neighbor_result.expect("should resolve").map(|n| n.id()),
            Some(expected_neighbor.id())
        );
    }

    /// A single in-flight `send_link_request` call resolves once its correlated
    /// `SetLinkOp` reply arrives, and that reply's `linked` identity is applied to
    /// this node's own lookup table via the arm's own write path (not by
    /// `send_link_request` itself).
    #[tokio::test]
    async fn test_send_link_request_resolves() {
        let id = random_identifier();
        let mem_vec = random_membership_vector();
        let span = span_fixture();
        let dest = random_identifier();
        let candidate = random_identity();
        let linked_identity = random_identity();
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
                .answers_arc(Arc::new(move |_, event_dest: Identifier, event: Event| {
                    assert_eq!(event_dest, dest, "expected request sent to dest");
                    match event {
                        GetLinkOp(req) => {
                            assert_eq!(req.side, Direction::Right);
                            assert_eq!(req.level, 0);
                            *nonce_mock.lock().expect("mutex poisoned") = Some(req.nonce);
                            Ok(())
                        }
                        _ => panic!("unexpected event: {:?}", event),
                    }
                }))
                .once(),
        ));

        let lt = ArrayLookupTable::new();
        let core = Box::new(BaseCore::new(
            span.clone(),
            id,
            mem_vec,
            Box::new(lt.clone()),
        ));
        let node = BaseNode::new(span, core, Box::new(mock_net)).expect("failed to create node");
        let node_reply = node.clone();

        let (send_result, ()) = tokio::time::timeout(Duration::from_secs(2), async {
            tokio::join!(
                node.send_link_request(
                    dest,
                    candidate,
                    Direction::Right,
                    0,
                    Duration::from_millis(200)
                ),
                async {
                    // captured synchronously by the mock before send_link_request's
                    // first await.
                    let nonce = nonce_cell
                        .lock()
                        .expect("mutex poisoned")
                        .expect("nonce should already be captured");
                    node_reply
                        .process_incoming_event(
                            dest,
                            SetLinkOp(LinkRes {
                                nonce,
                                side: Direction::Right,
                                level: 0,
                                linked: Some(linked_identity),
                            }),
                        )
                        .expect("failed to process reply");
                }
            )
        })
        .await
        .expect("test timed out");

        send_result.expect("should resolve");
        assert_eq!(
            lt.get_entry(0, Direction::Right)
                .expect("get_entry should not error")
                .map(|i| i.id()),
            Some(linked_identity.id()),
            "the SetLinkOp arm should have applied the link to this node's own table"
        );
    }

    /// A `SetLinkOp` whose `level` is out of range for the lookup table (a
    /// peer-controlled value, since `SetLinkOp` may arrive unsolicited with no
    /// accompanying local request to validate it against) is rejected without
    /// erroring the whole event and without touching the lookup table, while any
    /// pending `Waiter::Link` at that nonce is still resolved: the table-write and
    /// the waiter-resolution are independent paths.
    #[tokio::test]
    async fn test_set_link_op_rejects_out_of_range_level() {
        let id = random_identifier();
        let mem_vec = random_membership_vector();
        let span = span_fixture();
        let origin = random_identifier();
        let linked_identity = random_identity();

        let mock_net = Unimock::new((
            NetworkMock::register_processor
                .each_call(matching!(_))
                .answers(&|_, _| Ok(())),
            NetworkMock::clone_box
                .each_call(matching!())
                .answers(&|mock| Box::new(mock.clone())),
        ));

        let lt = ArrayLookupTable::new();
        let core = Box::new(BaseCore::new(
            span.clone(),
            id,
            mem_vec,
            Box::new(lt.clone()),
        ));
        let node = BaseNode::new(span, core, Box::new(mock_net)).expect("failed to create node");

        let nonce = Nonce::random();
        let (tx, rx) = oneshot::channel::<LinkRes>();
        node.request_id_map
            .lock()
            .expect("mutex poisoned")
            .insert(nonce, Waiter::Link(tx));

        let result = node.process_incoming_event(
            origin,
            SetLinkOp(LinkRes {
                nonce,
                side: Direction::Left,
                level: LOOKUP_TABLE_LEVELS,
                linked: Some(linked_identity),
            }),
        );
        assert!(
            result.is_ok(),
            "an out-of-range level must not error the whole event"
        );

        let resolved = tokio::time::timeout(Duration::from_secs(1), rx)
            .await
            .expect("test timed out")
            .expect("the waiter must still resolve regardless of the rejected write");
        assert_eq!(resolved.nonce, nonce);

        assert_eq!(
            lt.get_entry(0, Direction::Left)
                .expect("get_entry should not error"),
            None,
            "the out-of-range write must never reach the lookup table"
        );
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

    /// Aborting a `get_max_level` task mid-flight (before any reply is ever delivered)
    /// still cleans up its waiter-map entry. Unlike the timeout and resolve paths, this
    /// exercises `WaiterGuard`'s drop-on-cancellation cleanup specifically:
    /// `JoinHandle::abort` drops the future mid-`.await` without running any of
    /// `get_max_level`'s own branch code, so only `Drop` can be responsible for the
    /// removal here.
    #[tokio::test]
    async fn test_get_max_level_cleans_up_on_cancellation() {
        let id = random_identifier();
        let mem_vec = random_membership_vector();
        let span = span_fixture();
        let introducer = random_identifier();
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
        let node_task = node.clone();

        // no reply is ever delivered for this nonce: the task is cancelled instead.
        let handle = tokio::spawn(async move {
            node_task
                .get_max_level(introducer, Duration::from_secs(30))
                .await
        });

        // deterministic wait for registration: poll the shared map itself rather than
        // just the nonce capture, so this actually confirms what `WaiterGuard` is about
        // to clean up is present.
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let registered = nonce_cell.lock().expect("mutex poisoned").is_some()
                    && !node
                        .request_id_map
                        .lock()
                        .expect("mutex poisoned")
                        .is_empty();
                if registered {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("test timed out waiting for the waiter to register");

        handle.abort();

        // aborting doesn't run the cancelled future's drop glue synchronously; it runs
        // the next time the runtime polls the task. bounded poll, not a wall-clock
        // sleep, per this project's timeout-every-async-wait rule.
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if node
                    .request_id_map
                    .lock()
                    .expect("mutex poisoned")
                    .is_empty()
                {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("test timed out waiting for the waiter map entry to be cleaned up after abort");

        let join_result = tokio::time::timeout(Duration::from_secs(2), handle)
            .await
            .expect("test timed out waiting for the aborted task to join");
        assert!(
            join_result
                .expect_err("aborted task should yield a join error")
                .is_cancelled(),
            "expected the join error to report cancellation"
        );
    }

    /// With `introducer.id() < u.id()` (the search-right branch) and both `s` and `z`
    /// present, both `GetLinkOp`s are answered with a confirming `SetLinkOp`, and
    /// `join_stage1_link_level0` resolves once each reply is applied to this node's
    /// own table. The `s`-reply lands on the left slot, the `z`-reply on the right,
    /// per the documented asymmetry.
    #[tokio::test]
    async fn test_join_stage1_link_level0_links_both_sides() {
        let node_id = random_identifier();
        let mem_vec = random_membership_vector();
        let address = random_address();
        let span = span_fixture();
        let introducer = random_identifier_less_than(&node_id);
        let s_identity = random_identity();
        let z_identity = random_identity();
        let s_id = s_identity.id();
        let z_id = z_identity.id();

        let search_nonce_cell: Arc<Mutex<Option<Nonce>>> = Arc::new(Mutex::new(None));
        let neighbor_nonce_cell: Arc<Mutex<Option<Nonce>>> = Arc::new(Mutex::new(None));
        let s_link_nonce_cell: Arc<Mutex<Option<Nonce>>> = Arc::new(Mutex::new(None));
        let z_link_nonce_cell: Arc<Mutex<Option<Nonce>>> = Arc::new(Mutex::new(None));
        let (search_mock, neighbor_mock, s_link_mock, z_link_mock) = (
            search_nonce_cell.clone(),
            neighbor_nonce_cell.clone(),
            s_link_nonce_cell.clone(),
            z_link_nonce_cell.clone(),
        );

        let mock_net = Unimock::new((
            NetworkMock::register_processor
                .each_call(matching!(_))
                .answers(&|_, _| Ok(())),
            NetworkMock::clone_box
                .each_call(matching!())
                .answers(&|mock| Box::new(mock.clone())),
            NetworkMock::address
                .each_call(matching!())
                .answers_arc(Arc::new(move |_| address)),
            NetworkMock::send_event
                .each_call(matching!(_))
                .answers_arc(Arc::new(move |_, dest: Identifier, event: Event| {
                    match event {
                        SearchByIdRequest(req) => {
                            assert_eq!(dest, introducer, "search must go to the introducer");
                            assert_eq!(
                                req.direction,
                                Direction::Right,
                                "introducer.id() < u.id() must search Direction::Right"
                            );
                            *search_mock.lock().expect("mutex poisoned") = Some(req.nonce);
                        }
                        GetNeighborOp(req) => {
                            assert_eq!(dest, s_id, "neighbor query must go to s");
                            assert_eq!(
                                req.direction,
                                Direction::Right,
                                "neighbor query must reuse the search's own direction"
                            );
                            *neighbor_mock.lock().expect("mutex poisoned") = Some(req.nonce);
                        }
                        GetLinkOp(req) if dest == s_id => {
                            assert_eq!(
                                req.side,
                                Direction::Right,
                                "s's side must be Direction::Right"
                            );
                            *s_link_mock.lock().expect("mutex poisoned") = Some(req.nonce);
                        }
                        GetLinkOp(req) if dest == z_id => {
                            assert_eq!(
                                req.side,
                                Direction::Left,
                                "z's side must be Direction::Left"
                            );
                            *z_link_mock.lock().expect("mutex poisoned") = Some(req.nonce);
                        }
                        _ => panic!("unexpected event to {:?}: {:?}", dest, event),
                    }
                    Ok(())
                })),
        ));

        let lt = ArrayLookupTable::new();
        let core = Box::new(BaseCore::new(
            span.clone(),
            node_id,
            mem_vec,
            Box::new(lt.clone()),
        ));
        let node = BaseNode::new(span, core, Box::new(mock_net)).expect("failed to create node");
        let node_reply = node.clone();

        let deliver = async {
            let search_nonce = loop {
                if let Some(n) = *search_nonce_cell.lock().expect("mutex poisoned") {
                    break n;
                }
                tokio::task::yield_now().await;
            };
            node_reply
                .process_incoming_event(
                    introducer,
                    SearchByIdResponse(IdSearchRes {
                        nonce: search_nonce,
                        target: node_id,
                        termination_level: 0,
                        result: s_id,
                    }),
                )
                .expect("failed to process search reply");

            let neighbor_nonce = loop {
                if let Some(n) = *neighbor_nonce_cell.lock().expect("mutex poisoned") {
                    break n;
                }
                tokio::task::yield_now().await;
            };
            node_reply
                .process_incoming_event(
                    s_id,
                    RetNeighborOp(NeighborRes {
                        nonce: neighbor_nonce,
                        level: 0,
                        direction: Direction::Right,
                        neighbor: Some(z_identity),
                    }),
                )
                .expect("failed to process neighbor reply");

            let (s_link_nonce, z_link_nonce) = loop {
                let s = *s_link_nonce_cell.lock().expect("mutex poisoned");
                let z = *z_link_nonce_cell.lock().expect("mutex poisoned");
                if let (Some(s), Some(z)) = (s, z) {
                    break (s, z);
                }
                tokio::task::yield_now().await;
            };
            node_reply
                .process_incoming_event(
                    s_id,
                    SetLinkOp(LinkRes {
                        nonce: s_link_nonce,
                        side: Direction::Left,
                        level: 0,
                        linked: Some(s_identity),
                    }),
                )
                .expect("failed to process s link reply");
            node_reply
                .process_incoming_event(
                    z_id,
                    SetLinkOp(LinkRes {
                        nonce: z_link_nonce,
                        side: Direction::Right,
                        level: 0,
                        linked: Some(z_identity),
                    }),
                )
                .expect("failed to process z link reply");
        };

        let (join_result, ()) = tokio::time::timeout(Duration::from_secs(2), async {
            tokio::join!(
                node.join_stage1_link_level0(introducer, 0, Duration::from_secs(1)),
                deliver
            )
        })
        .await
        .expect("test timed out");

        join_result.expect("join_stage1_link_level0 should resolve");

        assert_eq!(
            lt.get_entry(0, Direction::Left)
                .expect("get_entry should not error")
                .map(|identity| identity.id()),
            Some(s_id),
            "s's reply must land on this node's own left slot"
        );
        assert_eq!(
            lt.get_entry(0, Direction::Right)
                .expect("get_entry should not error")
                .map(|identity| identity.id()),
            Some(z_id),
            "z's reply must land on this node's own right slot"
        );
    }
}
