use crate::core::model::search::Nonce;
use crate::core::{
    Direction, IdSearchReq, IdSearchRes, Identifier, Identity, IrrevocableContext, LinkOutcome,
    LinkReq, LinkRes, LookupTableLevel, MaxLevelReq, MaxLevelRes, MembershipVector, NeighborReq,
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

    /// Returns this node's own full identity. Lives on `BaseNode`, not `Core`, since
    /// `Core` knows nothing about the network and has no address to contribute.
    fn self_identity(&self) -> Identity {
        Identity::new(self.core.id(), self.core.mem_vec(), self.net.address())
    }

    #[tracing::instrument(level = "trace", parent = &self.span, fields(target = ?req.target, level = ?req.level), skip(self, req, timeout))]
    pub(crate) async fn search_by_id(
        &self,
        req: IdSearchReq,
        timeout: Duration,
    ) -> anyhow::Result<IdSearchRes> {
        tracing::trace!("searching for target {:?}", req.target);
        let local_res = self
            .core
            .search_by_id(req)
            .map_err(|e| anyhow!("failed to perform search by id {}", e))?;
        if local_res.result == self.core.id() {
            tracing::trace!("found self in search by id, terminating the search result");
            return Ok(local_res);
        }

        self.send_search_by_id_req(
            local_res.result,
            req.nonce,
            req.target,
            local_res.termination_level,
            req.direction,
            timeout,
        )
        .await
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

    /// Sends the request to `dest` and waits for the terminal node's reply, skipping this
    /// node's own local search. `dest` must not be past `target` in `direction`, so
    /// `dest <= target` for `Right` and `dest >= target` for `Left`. A node with no better
    /// neighbor answers with its own identifier, which is valid only under that condition.
    /// Each relay hop preserves it, so the caller guarantees it for the first hop only.
    ///
    /// # Returns
    ///
    /// The terminal node's response.
    ///
    /// # Errors
    ///
    /// * **RECOVERABLE, INTERNAL.** Send failure, dropped reply channel, or `timeout`.
    #[tracing::instrument(level = "trace", parent = &self.span, skip(self))]
    async fn send_search_by_id_req(
        &self,
        dest: Identifier,
        nonce: Nonce,
        target: Identifier,
        level: LookupTableLevel,
        direction: Direction,
        timeout: Duration,
    ) -> anyhow::Result<IdSearchRes> {
        let (tx, rx) = oneshot::channel::<IdSearchRes>();
        self.request_id_map
            .lock()
            .expect("mutex was poisoned by a previous panic")
            .insert(nonce, Waiter::AsyncSearch(tx));
        let _guard = WaiterGuard::new(nonce, self.request_id_map.clone());

        let req = IdSearchReq {
            nonce,
            target,
            origin: self.core.id(),
            level,
            direction,
        };
        self.net
            .send_event(dest, SearchByIdRequest(req))
            .map_err(|e| anyhow!("failed to send search by id request: {}", e))?;
        tracing::info!("sent search by id request, pending response");

        tokio::time::timeout(timeout, rx)
            .await
            .map_err(|_| anyhow!("timed out waiting for search by id response"))?
            .map_err(|_| anyhow!("failed to receive search by id response: sender dropped"))
    }

    /// Asks `from` for its current neighbor entry on `direction` at `level`.
    ///
    /// # Args
    ///
    /// * `from`, the node to query.
    /// * `direction`, which of `from`'s own slots to query.
    /// * `level`, which lookup-table level to query.
    /// * `timeout`, how long to wait for `from`'s reply before giving up.
    ///
    /// # Returns
    ///
    /// `from`'s current neighbor on `direction` at `level`, or `None` if `from`
    /// currently believes it has none there.
    ///
    /// # Errors
    ///
    /// * **RECOVERABLE, INTERNAL.** Sending the request fails, the reply channel is
    ///   dropped, or `timeout` elapses before a reply arrives.
    #[tracing::instrument(
        parent = &self.span,
        fields(from = ?from, direction = ?direction, level = ?level),
        skip(self, timeout)
    )]
    pub(crate) async fn get_neighbor(
        &self,
        from: Identifier,
        direction: Direction,
        level: LookupTableLevel,
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
                    level,
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

    /// Sends `GetLinkOp` to `dest`, asking it to adopt this node as its neighbor
    /// on `dir` at `level`, part of stage 1 of the join protocol.
    ///
    /// The resulting `SetLinkOp` reply is applied to this node's own lookup table by
    /// `handle_set_link_response`, not by this method. By the time this method's
    /// `await` resolves, that write has already happened, since both run on the same
    /// synchronous event-processing path.
    ///
    /// # Args
    ///
    /// * `dest`, the node to send the link request to.
    /// * `dir`, receiver-owned, which of `dest`'s own slots this node is
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
        fields(dest = ?dest, dir = ?dir, level = ?level),
        skip(self, timeout)
    )]
    async fn send_link_request(
        &self,
        dest: Identifier,
        dir: Direction,
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
                    candidate: self.self_identity(),
                    dir,
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

    /// Removes and returns the `nonce`-keyed waiter, but only when the map's current
    /// entry there matches `is_expected_variant`; a present but wrong-typed entry is
    /// left untouched rather than destroyed, since it may belong to a different
    /// in-flight request. Logs why, at warn level, whenever it returns `None`.
    ///
    /// # Args
    ///
    /// * `nonce` — the correlation id to look up.
    /// * `expected` — the variant name this caller expects, named only in the warning
    ///   logged on a mismatch.
    /// * `is_expected_variant` — tests the map's current entry without consuming it.
    fn take_waiter(
        &self,
        nonce: Nonce,
        expected: &str,
        is_expected_variant: impl Fn(&Waiter) -> bool,
    ) -> Option<Waiter> {
        let mut request_id_map = self
            .request_id_map
            .lock()
            .expect("mutex was poisoned by a previous panic");
        match request_id_map.get(&nonce) {
            Some(w) if is_expected_variant(w) => request_id_map.remove(&nonce),
            Some(x) => {
                tracing::warn!(
                    "invalid waiter in the map, expected Waiter::{}, got {:?}",
                    expected,
                    x
                );
                None
            }
            None => {
                tracing::warn!("no waiter exists in the map for that request_id");
                None
            }
        }
    }

    /// Handles an inbound `SearchByIdRequest`: performs the local search and either
    /// answers directly, when this node is the result, or relays the request to the
    /// next hop.
    #[tracing::instrument(
        level = "trace",
        parent = &tracing::Span::current(),
        fields(nonce = ?req.nonce, target = ?req.target, direction = ?req.direction, level = ?req.level),
        skip(self, req)
    )]
    fn handle_search_by_id_request(&self, req: IdSearchReq) -> anyhow::Result<()> {
        tracing::trace!("received request");

        let res = self
            .core
            .search_by_id(req)
            .map_err(|e| anyhow!("failed to perform search by id {}", e))?;

        let span = tracing::trace_span!(
            parent: &tracing::Span::current(),
            "terminating",
            result = ?res.result,
            termination_level = ?res.termination_level
        );
        let _enter = span.enter();

        if res.result == self.core.id() {
            self.net
                .send_event(req.origin, SearchByIdResponse(res))
                .map_err(|e| anyhow!("failed to send response event for search by id: {}", e))?;
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

    /// Handles an inbound `SearchByIdResponse`: resolves the correlated waiter, if any.
    #[tracing::instrument(
        level = "trace",
        parent = &tracing::Span::current(),
        fields(nonce = ?res.nonce, target = ?res.target, result = ?res.result, termination_level = ?res.termination_level),
        skip(self, res)
    )]
    fn handle_search_by_id_response(&self, res: IdSearchRes) -> anyhow::Result<()> {
        if let Some(Waiter::AsyncSearch(tx)) = self.take_waiter(res.nonce, "AsyncSearch", |w| {
            matches!(w, Waiter::AsyncSearch(_))
        }) {
            if let Err(e) = tx.send(res) {
                tracing::warn!("failed to send the response to the receiver end: {:?}", e)
            }
        }
        Ok(())
    }

    /// Handles an inbound `GetMaxLevelOp`: reads the local lookup table and replies
    /// with `RetMaxLevelOp`.
    #[tracing::instrument(
        level = "trace",
        parent = &tracing::Span::current(),
        fields(nonce = ?req.nonce),
        skip(self, req)
    )]
    fn handle_get_max_level_request(&self, req: MaxLevelReq) -> anyhow::Result<()> {
        let max_level = self
            .core
            .max_level()
            .map_err(|e| anyhow!("failed to read local max level: {}", e))?;

        self.net
            .send_event(
                req.origin,
                RetMaxLevelOp(MaxLevelRes {
                    nonce: req.nonce,
                    max_level,
                }),
            )
            .map_err(|e| anyhow!("failed to send max level response: {}", e))?;
        tracing::info!("answered get max level request with {:?}", max_level);

        Ok(())
    }

    /// Handles an inbound `RetMaxLevelOp`: resolves the correlated waiter, if any.
    #[tracing::instrument(
        level = "trace",
        parent = &tracing::Span::current(),
        fields(nonce = ?res.nonce, max_level = ?res.max_level),
        skip(self, res)
    )]
    fn handle_ret_max_level_response(&self, res: MaxLevelRes) -> anyhow::Result<()> {
        if let Some(Waiter::MaxLevel(tx)) =
            self.take_waiter(res.nonce, "MaxLevel", |w| matches!(w, Waiter::MaxLevel(_)))
        {
            if let Err(e) = tx.send(res) {
                tracing::warn!("failed to send the response to the receiver end: {:?}", e)
            }
        }
        Ok(())
    }

    /// Handles an inbound `GetNeighborOp`: reads the local lookup table and replies
    /// with `RetNeighborOp`.
    #[tracing::instrument(
        level = "trace",
        parent = &tracing::Span::current(),
        fields(nonce = ?req.nonce, level = ?req.level, direction = ?req.direction),
        skip(self, req)
    )]
    fn handle_get_neighbor_request(&self, req: NeighborReq) -> anyhow::Result<()> {
        let neighbor = self
            .core
            .neighbor_entry(req.level, req.direction)
            .map_err(|e| anyhow!("failed to read local neighbor entry: {}", e))?;

        self.net
            .send_event(
                req.origin,
                RetNeighborOp(NeighborRes {
                    nonce: req.nonce,
                    level: req.level,
                    direction: req.direction,
                    neighbor,
                }),
            )
            .map_err(|e| anyhow!("failed to send neighbor response: {}", e))?;
        tracing::info!("answered get neighbor request with {:?}", neighbor);

        Ok(())
    }

    /// Handles an inbound `RetNeighborOp`: resolves the correlated waiter, if any.
    #[tracing::instrument(
        level = "trace",
        parent = &tracing::Span::current(),
        fields(nonce = ?res.nonce, level = ?res.level, direction = ?res.direction, neighbor = ?res.neighbor),
        skip(self, res)
    )]
    fn handle_ret_neighbor_response(&self, res: NeighborRes) -> anyhow::Result<()> {
        if let Some(Waiter::Neighbor(tx)) =
            self.take_waiter(res.nonce, "Neighbor", |w| matches!(w, Waiter::Neighbor(_)))
        {
            if let Err(e) = tx.send(res) {
                tracing::warn!("failed to send the response to the receiver end: {:?}", e)
            }
        }
        Ok(())
    }

    /// Handles an inbound `GetLinkOp`: either links the candidate directly, when
    /// this node's own slot at `(level, dir)` is the correct place for it, or
    /// forwards the request unchanged to whichever existing neighbor sits between
    /// them, mirroring `Core::try_link`'s decision.
    #[tracing::instrument(
        level = "trace",
        parent = &tracing::Span::current(),
        fields(nonce = ?req.nonce, candidate = ?req.candidate, dir = ?req.dir, level = ?req.level),
        skip(self, req)
    )]
    fn handle_get_link_request(&self, req: LinkReq) -> anyhow::Result<()> {
        // `req.level` is peer-controlled with no accompanying local request to
        // sanity-check it against, the same boundary `handle_set_link_response`
        // already guards below for the identical reason: an out-of-range level here
        // means a malformed or adversarial peer message, not this node's own broken
        // invariant, so it's logged and dropped rather than answered or propagated
        // as a hard error.
        if req.level >= LOOKUP_TABLE_LEVELS {
            tracing::warn!(
                "rejected get link op from peer with out-of-range level {}",
                req.level
            );
            return Ok(());
        }

        match self
            .core
            .try_link(req.level, req.dir, req.candidate)
            .map_err(|e| anyhow!("failed to decide link at level {}: {}", req.level, e))?
        {
            LinkOutcome::LinkedDirectly => {
                let linked = self.self_identity();
                self.net
                    .send_event(
                        req.candidate.id(),
                        SetLinkOp(LinkRes {
                            nonce: req.nonce,
                            dir: req.dir.opposite(),
                            level: req.level,
                            linked: Some(linked),
                        }),
                    )
                    .map_err(|e| anyhow!("failed to send link response: {}", e))?;
                tracing::info!("linked candidate directly at level {}", req.level);
            }
            LinkOutcome::Forward(existing) => {
                self.net
                    .send_event(existing.id(), GetLinkOp(req))
                    .map_err(|e| anyhow!("failed to forward get link request: {}", e))?;
                tracing::info!(
                    "forwarded get link request to existing neighbor at level {}",
                    req.level
                );
            }
        }
        Ok(())
    }

    /// Handles an inbound `SetLinkOp`: applies the link to this node's own lookup
    /// table, then resolves the correlated waiter, if any.
    #[tracing::instrument(
        level = "trace",
        parent = &tracing::Span::current(),
        fields(nonce = ?res.nonce, dir = ?res.dir, level = ?res.level, linked = ?res.linked),
        skip(self, res)
    )]
    fn handle_set_link_response(&self, res: LinkRes) -> anyhow::Result<()> {
        // the write runs whether or not a waiter matches below. a `SetLinkOp` can
        // also arrive unsolicited as a repair push, and that correction must land
        // in the table too.
        //
        // `res.level` comes from a peer, and no local request exists to check it
        // against. `Core::try_link` assumes a known-good level, so this arm
        // range-checks it first and treats a bad one as a malformed peer message
        // rather than a local invariant violation.
        if let Some(linked) = res.linked {
            if res.level >= LOOKUP_TABLE_LEVELS {
                tracing::warn!(
                    "rejected set link op from peer with out-of-range level {}",
                    res.level
                );
            } else {
                self.core
                    .try_link(res.level, res.dir, linked)
                    .map_err(|e| anyhow!("failed to apply link at level {}: {}", res.level, e))?;
                tracing::trace!("applied link to own lookup table");
            }
        }

        if let Some(Waiter::Link(tx)) =
            self.take_waiter(res.nonce, "Link", |w| matches!(w, Waiter::Link(_)))
        {
            if let Err(e) = tx.send(res) {
                tracing::warn!("failed to send the response to the receiver end: {:?}", e)
            }
        }
        Ok(())
    }
}

impl EventProcessorCore for BaseNode {
    #[tracing::instrument(
        level = "trace",
        parent = &self.span,
        fields(origin = ?origin_id),
        skip(self, event)
    )]
    fn process_incoming_event(&self, origin_id: Identifier, event: Event) -> anyhow::Result<()> {
        match event {
            SearchByIdRequest(req) => self.handle_search_by_id_request(req),
            SearchByIdResponse(res) => self.handle_search_by_id_response(res),
            GetMaxLevelOp(req) => self.handle_get_max_level_request(req),
            RetMaxLevelOp(res) => self.handle_ret_max_level_response(res),
            GetNeighborOp(req) => self.handle_get_neighbor_request(req),
            RetNeighborOp(res) => self.handle_ret_neighbor_response(res),
            GetLinkOp(req) => self.handle_get_link_request(req),
            SetLinkOp(res) => self.handle_set_link_response(res),
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
        random_identifier_less_than, random_identity, random_membership_vector,
        random_sorted_identifiers, span_fixture,
    };
    use crate::core::{ArrayLookupTable, LookupTable};
    use crate::network::mock::hub::NetworkHub;
    use crate::network::NetworkMock;
    use crate::node::core::BaseCore;
    use crate::node::testutil::make_core;
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

    /// A single in-flight `send_search_by_id_req` call resolves to the result of its
    /// correlated `SearchByIdResponse` and sends the caller-supplied `direction` and `level`.
    #[tokio::test]
    async fn test_send_search_by_id_req_resolves() {
        let id = random_identifier();
        let introducer = random_identifier();
        let expected_anchor = random_identifier();
        let search_level: LookupTableLevel = 3;
        let nonce = Nonce::random();

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
                    let SearchByIdRequest(req) = event else {
                        panic!("unexpected event: {:?}", event)
                    };
                    assert_eq!(req.direction, Direction::Right);
                    assert_eq!(req.level, search_level);
                    Ok(())
                }))
                .once(),
        ));

        let core = Box::new(make_core(id, Box::new(ArrayLookupTable::new())));
        let node =
            BaseNode::new(span_fixture(), core, Box::new(mock_net)).expect("failed to create node");

        let (res, ()) = tokio::join!(
            node.send_search_by_id_req(
                introducer,
                nonce,
                id,
                search_level,
                Direction::Right,
                Duration::from_millis(200)
            ),
            async {
                node.process_incoming_event(
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
        );

        assert_eq!(res.expect("should resolve").result, expected_anchor);
    }

    /// A single in-flight `get_neighbor` call resolves to the neighbor entry carried
    /// by its correlated `RetNeighborOp` reply, and the outbound request carries the
    /// caller-supplied `direction` and `level`.
    #[tokio::test]
    async fn test_get_neighbor_resolves() {
        let id = random_identifier();
        let mem_vec = random_membership_vector();
        let span = span_fixture();
        let from = random_identifier();
        let expected_neighbor = random_identity();
        let query_level: LookupTableLevel = 3;
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
                            assert_eq!(req.level, query_level);
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
                node.get_neighbor(
                    from,
                    Direction::Right,
                    query_level,
                    Duration::from_millis(200)
                ),
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
                                level: query_level,
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

    /// A `get_neighbor` call whose correlated `RetNeighborOp` reply carries no neighbor
    /// resolves to `Ok(None)`, the "queried node has none there" case `Option<Identity>`
    /// exists to carry.
    #[tokio::test]
    async fn test_get_neighbor_resolves_to_none() {
        let id = random_identifier();
        let mem_vec = random_membership_vector();
        let span = span_fixture();
        let from = random_identifier();
        let query_level: LookupTableLevel = 2;
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
                node.get_neighbor(
                    from,
                    Direction::Right,
                    query_level,
                    Duration::from_millis(200)
                ),
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
                                level: query_level,
                                direction: Direction::Right,
                                neighbor: None,
                            }),
                        )
                        .expect("failed to process reply");
                }
            )
        })
        .await
        .expect("test timed out");

        assert_eq!(neighbor_result.expect("should resolve"), None);
    }

    /// `process_incoming_event` answers a `GetMaxLevelOp` request by reading the local
    /// lookup table and replying with `RetMaxLevelOp`, the responder side of the round
    /// trip `get_max_level` drives from the requester side.
    #[test]
    fn test_process_incoming_event_answers_get_max_level_request() {
        let id = random_identifier();
        let mem_vec = random_membership_vector();
        let span = span_fixture();
        let origin = random_identifier();
        let expected_level: LookupTableLevel = 4;

        let lt = ArrayLookupTable::new();
        lt.update_entry(random_identity(), expected_level, Direction::Right)
            .expect("failed to seed lookup table");

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
                    assert_eq!(dest, origin, "expected response sent back to the requester");
                    let RetMaxLevelOp(res) = event else {
                        panic!("unexpected event: {:?}", event)
                    };
                    assert_eq!(res.max_level, expected_level);
                    Ok(())
                }))
                .once(),
        ));

        let core = Box::new(BaseCore::new(span.clone(), id, mem_vec, Box::new(lt)));
        let node = BaseNode::new(span, core, Box::new(mock_net)).expect("failed to create node");

        node.process_incoming_event(
            origin,
            GetMaxLevelOp(MaxLevelReq {
                nonce: Nonce::random(),
                origin,
            }),
        )
        .expect("failed to answer get max level request");
    }

    /// `process_incoming_event` answers a `GetNeighborOp` request by reading the local
    /// lookup table and replying with `RetNeighborOp`, the responder side of the round
    /// trip `get_neighbor` drives from the requester side.
    #[test]
    fn test_process_incoming_event_answers_get_neighbor_request() {
        let id = random_identifier();
        let mem_vec = random_membership_vector();
        let span = span_fixture();
        let origin = random_identifier();
        let expected_neighbor = random_identity();

        let lt = ArrayLookupTable::new();
        lt.update_entry(expected_neighbor, 0, Direction::Right)
            .expect("failed to seed lookup table");

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
                    assert_eq!(dest, origin, "expected response sent back to the requester");
                    let RetNeighborOp(res) = event else {
                        panic!("unexpected event: {:?}", event)
                    };
                    assert_eq!(res.level, 0);
                    assert_eq!(res.direction, Direction::Right);
                    assert_eq!(res.neighbor.map(|n| n.id()), Some(expected_neighbor.id()));
                    Ok(())
                }))
                .once(),
        ));

        let core = Box::new(BaseCore::new(span.clone(), id, mem_vec, Box::new(lt)));
        let node = BaseNode::new(span, core, Box::new(mock_net)).expect("failed to create node");

        node.process_incoming_event(
            origin,
            GetNeighborOp(NeighborReq {
                nonce: Nonce::random(),
                origin,
                level: 0,
                direction: Direction::Right,
            }),
        )
        .expect("failed to answer get neighbor request");
    }

    /// `process_incoming_event` answers a `GetLinkOp` whose requested slot is empty
    /// by linking the candidate directly and replying to it with `SetLinkOp` carrying
    /// this node's own identity, the responder side of the round trip
    /// `send_link_request` drives from the requester side.
    #[test]
    fn test_process_incoming_event_links_get_link_request_directly() {
        let id = random_identifier();
        let mem_vec = random_membership_vector();
        let span = span_fixture();
        let address = random_address();
        let candidate = random_identity();

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
                    assert_eq!(
                        dest,
                        candidate.id(),
                        "expected reply sent back to the candidate"
                    );
                    let SetLinkOp(res) = event else {
                        panic!("unexpected event: {:?}", event)
                    };
                    assert_eq!(res.dir, Direction::Left);
                    assert_eq!(res.level, 0);
                    assert_eq!(res.linked.map(|i| i.id()), Some(id));
                    Ok(())
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

        node.process_incoming_event(
            candidate.id(),
            GetLinkOp(LinkReq {
                nonce: Nonce::random(),
                candidate,
                dir: Direction::Right,
                level: 0,
            }),
        )
        .expect("failed to answer get link request");
    }

    /// A `GetLinkOp` whose requested slot already holds a neighbor strictly between
    /// this node and the candidate is forwarded, unchanged, to that neighbor instead
    /// of being answered directly, mirroring `Core::try_link`'s `Forward` outcome.
    #[test]
    fn test_process_incoming_event_forwards_get_link_request() {
        let id = random_identifier();
        let mem_vec = random_membership_vector();
        let span = span_fixture();
        let candidate = random_identity();
        let existing = Identity::new(
            random_identifier_less_than(&candidate.id()),
            random_membership_vector(),
            random_address(),
        );

        let lt = ArrayLookupTable::new();
        lt.update_entry(existing, 0, Direction::Right)
            .expect("failed to seed lookup table");

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
                    assert_eq!(
                        dest,
                        existing.id(),
                        "expected the request forwarded to the closer neighbor"
                    );
                    let GetLinkOp(req) = event else {
                        panic!("unexpected event: {:?}", event)
                    };
                    assert_eq!(req.candidate.id(), candidate.id());
                    assert_eq!(req.dir, Direction::Right);
                    assert_eq!(req.level, 0);
                    Ok(())
                }))
                .once(),
        ));

        let core = Box::new(BaseCore::new(span.clone(), id, mem_vec, Box::new(lt)));
        let node = BaseNode::new(span, core, Box::new(mock_net)).expect("failed to create node");

        node.process_incoming_event(
            candidate.id(),
            GetLinkOp(LinkReq {
                nonce: Nonce::random(),
                candidate,
                dir: Direction::Right,
                level: 0,
            }),
        )
        .expect("failed to forward get link request");
    }

    /// A single in-flight `send_link_request` call resolves once its correlated
    /// `SetLinkOp` reply arrives, and that reply's `linked` identity is applied to
    /// this node's own lookup table via the arm's own write path (not by
    /// `send_link_request` itself). The outbound request carries this node's own
    /// identity as `candidate`.
    #[tokio::test]
    async fn test_send_link_request_resolves() {
        let id = random_identifier();
        let mem_vec = random_membership_vector();
        let span = span_fixture();
        let address = random_address();
        let dest = random_identifier();
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
            NetworkMock::address
                .each_call(matching!())
                .answers_arc(Arc::new(move |_| address)),
            NetworkMock::send_event
                .each_call(matching!(_))
                .answers_arc(Arc::new(move |_, event_dest: Identifier, event: Event| {
                    assert_eq!(event_dest, dest, "expected request sent to dest");
                    match event {
                        GetLinkOp(req) => {
                            assert_eq!(
                                req.candidate,
                                Identity::new(id, mem_vec, address),
                                "expected the request to carry this node's own identity"
                            );
                            assert_eq!(req.dir, Direction::Right);
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
                node.send_link_request(dest, Direction::Right, 0, Duration::from_millis(200)),
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
                                dir: Direction::Right,
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

    /// A full `GetLinkOp`/`SetLinkOp` round trip over the mock hub leaves both nodes'
    /// own tables ordered correctly. The candidate asks the responder, which holds the
    /// smaller identifier, to install the candidate in the responder's own right slot,
    /// and the responder's reply must land in the candidate's own left slot rather than
    /// its right one. A reply naming the candidate's right slot would put a smaller
    /// identifier there, breaking the ordering invariant, and `try_link` on the
    /// candidate cannot reject that write because the slot it lands in is empty.
    #[tokio::test]
    async fn test_link_round_trip_fills_the_candidates_mirror_slot() {
        let hub = NetworkHub::new();
        let responder_id = random_identifier();
        let candidate_id = random_identifier_greater_than(&responder_id);

        let responder_lt = ArrayLookupTable::new();
        let candidate_lt = ArrayLookupTable::new();
        let responder_net =
            NetworkHub::new_mock_network(hub.clone(), responder_id, random_address())
                .expect("failed to create the responder's network");
        let candidate_net =
            NetworkHub::new_mock_network(hub.clone(), candidate_id, random_address())
                .expect("failed to create the candidate's network");

        // the responder's own handle is never used directly. `BaseNode::new` registers a
        // clone of it on its network, and that clone is what answers the inbound request.
        let _responder = BaseNode::new(
            span_fixture(),
            Box::new(make_core(responder_id, Box::new(responder_lt.clone()))),
            responder_net.clone_box(),
        )
        .expect("failed to create the responder node");
        let candidate = BaseNode::new(
            span_fixture(),
            Box::new(make_core(candidate_id, Box::new(candidate_lt.clone()))),
            candidate_net.clone_box(),
        )
        .expect("failed to create the candidate node");

        tokio::time::timeout(
            Duration::from_secs(2),
            candidate.send_link_request(
                responder_id,
                Direction::Right,
                0,
                Duration::from_millis(200),
            ),
        )
        .await
        .expect("test timed out")
        .expect("the link request should resolve");

        assert_eq!(
            responder_lt
                .get_entry(0, Direction::Right)
                .expect("get_entry should not error")
                .map(|i| i.id()),
            Some(candidate_id),
            "the responder must hold the candidate in the slot it was asked to fill"
        );
        assert_eq!(
            candidate_lt
                .get_entry(0, Direction::Left)
                .expect("get_entry should not error")
                .map(|i| i.id()),
            Some(responder_id),
            "the candidate's own left slot must hold the responder"
        );
        assert_eq!(
            candidate_lt
                .get_entry(0, Direction::Right)
                .expect("get_entry should not error"),
            None,
            "the candidate's own right slot must stay empty, the responder's identifier is smaller"
        );
    }

    /// A `GetLinkOp` that forwards twice before any node accepts it still lands its
    /// reply in the original candidate's mirror slot, not the forwarding hop's. Four
    /// nodes link rightward through real round trips, each one asking the smallest
    /// node, so the last request walks past two already-linked nodes before the third
    /// accepts it, and the resulting chain must be reciprocal on every node.
    ///
    /// The bug class this catches is worse than one misplaced pointer. A reply naming
    /// the unmirrored slot puts a smaller identifier in the candidate's own right
    /// slot, and the next request then forwards between those two nodes without
    /// bound, since each one sees the other as closer to the new candidate. The mock
    /// hub dispatches re-entrantly, so an unbounded message count surfaces as an
    /// overflowed stack and aborts the test binary.
    #[tokio::test]
    async fn test_link_round_trip_forwards_twice_rightward() {
        let hub = NetworkHub::new();
        let ids = random_sorted_identifiers(4);
        let tables: Vec<ArrayLookupTable> =
            (0..ids.len()).map(|_| ArrayLookupTable::new()).collect();
        let nodes: Vec<BaseNode> = ids
            .iter()
            .zip(tables.iter())
            .map(|(&id, lt)| {
                let net = NetworkHub::new_mock_network(hub.clone(), id, random_address())
                    .expect("failed to create a mock network");
                BaseNode::new(
                    span_fixture(),
                    Box::new(make_core(id, Box::new(lt.clone()))),
                    net.clone_box(),
                )
                .expect("failed to create a node")
            })
            .collect();

        // every candidate asks the smallest node, so its request forwards past every
        // node already linked to that node's right before some node accepts it.
        for candidate in nodes.iter().skip(1) {
            tokio::time::timeout(
                Duration::from_secs(2),
                candidate.send_link_request(
                    ids[0],
                    Direction::Right,
                    0,
                    Duration::from_millis(200),
                ),
            )
            .await
            .expect("test timed out")
            .expect("the link request should resolve");
        }

        for (i, lt) in tables.iter().enumerate() {
            let below = if i == 0 { None } else { Some(ids[i - 1]) };
            let above = ids.get(i + 1).copied();
            assert_eq!(
                lt.get_entry(0, Direction::Left)
                    .expect("get_entry should not error")
                    .map(|e| e.id()),
                below,
                "node {i}'s own left slot must hold the node below it in the chain"
            );
            assert_eq!(
                lt.get_entry(0, Direction::Right)
                    .expect("get_entry should not error")
                    .map(|e| e.id()),
                above,
                "node {i}'s own right slot must hold the node above it in the chain"
            );
        }
    }

    /// The leftward mirror of the rightward two-forward case. Four nodes link
    /// leftward through real round trips, each one asking the largest node, so the
    /// last request walks past two already-linked nodes before the third accepts it.
    /// This is the other arm of `Direction::opposite`, and of `Core::try_link`'s
    /// `Forward` decision, so a one-sided mirror passes the rightward case and fails
    /// here.
    #[tokio::test]
    async fn test_link_round_trip_forwards_twice_leftward() {
        let hub = NetworkHub::new();
        let ids = random_sorted_identifiers(4);
        let tables: Vec<ArrayLookupTable> =
            (0..ids.len()).map(|_| ArrayLookupTable::new()).collect();
        let nodes: Vec<BaseNode> = ids
            .iter()
            .zip(tables.iter())
            .map(|(&id, lt)| {
                let net = NetworkHub::new_mock_network(hub.clone(), id, random_address())
                    .expect("failed to create a mock network");
                BaseNode::new(
                    span_fixture(),
                    Box::new(make_core(id, Box::new(lt.clone()))),
                    net.clone_box(),
                )
                .expect("failed to create a node")
            })
            .collect();

        // every candidate asks the largest node, so its request forwards past every
        // node already linked to that node's left before some node accepts it.
        let largest = ids.len() - 1;
        for candidate in nodes[..largest].iter().rev() {
            tokio::time::timeout(
                Duration::from_secs(2),
                candidate.send_link_request(
                    ids[largest],
                    Direction::Left,
                    0,
                    Duration::from_millis(200),
                ),
            )
            .await
            .expect("test timed out")
            .expect("the link request should resolve");
        }

        for (i, lt) in tables.iter().enumerate() {
            let below = if i == 0 { None } else { Some(ids[i - 1]) };
            let above = ids.get(i + 1).copied();
            assert_eq!(
                lt.get_entry(0, Direction::Left)
                    .expect("get_entry should not error")
                    .map(|e| e.id()),
                below,
                "node {i}'s own left slot must hold the node below it in the chain"
            );
            assert_eq!(
                lt.get_entry(0, Direction::Right)
                    .expect("get_entry should not error")
                    .map(|e| e.id()),
                above,
                "node {i}'s own right slot must hold the node above it in the chain"
            );
        }
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
                dir: Direction::Left,
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
    /// 1. The map's `Mutex` held across the `.await`, which
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
        let search_handle = tokio::spawn(async move {
            node_search
                .search_by_id(search_req, Duration::from_secs(30))
                .await
        });
        // deliberately generous: this budget is spent waiting for the spawned search task
        // to be scheduled, so a tight bound here fails under load. timeout
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
}
