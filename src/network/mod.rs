pub mod mock;
mod processor;

use crate::core::{
    BuddyReq, CheckNeighborReq, IdSearchReq, IdSearchRes, Identifier, LinkReq, LinkRes,
    MaxLevelReq, MaxLevelRes, NeighborReq, NeighborRes,
};
#[allow(unused)]
pub use processor::MessageProcessor;

/// Event enum defines the semantics of the event payload that are processed by the Skip Graph event processor.
/// Event is an application-layer semantic contrast to the lower-level transport-layer Message struct.
#[derive(Debug, Clone)]
pub enum Event {
    /// a payload for testing purposes: a simple string event, not used in production.
    TestMessage(String),
    /// a payload representing an identifier search request.
    SearchByIdRequest(IdSearchReq),
    /// a payload representing an identifier search response.
    SearchByIdResponse(IdSearchRes),
    /// a request for the receiver's highest populated lookup-table level.
    GetMaxLevelOp(MaxLevelReq),
    /// the response carrying the responder's highest populated lookup-table level.
    RetMaxLevelOp(MaxLevelRes),
    /// a request for the receiver's neighbor entry at a given level and direction.
    GetNeighborOp(NeighborReq),
    /// the response carrying the queried neighbor entry, if any.
    RetNeighborOp(NeighborRes),
    /// a forwardable request to link the candidate at the receiver's given side and level.
    GetLinkOp(LinkReq),
    /// a link confirmation reply; also reused as a repair push correction.
    SetLinkOp(LinkRes),
    /// a forwardable stage-2 request for a prefix-matching neighbor at a given level.
    BuddyOp(BuddyReq),
    /// a forwardable periodic repair probe checking backpointer consistency.
    CheckNeighborOp(CheckNeighborReq),
}

/// Core event processing logic that implementations must provide.
/// This trait is deliberately simple and doesn't require thread-safety concerns.
/// The EventProcessor wrapper handles all synchronization automatically.
pub trait EventProcessorCore: Send + Sync {
    /// Process an incoming event. This method will be called with proper synchronization.
    /// Arguments:
    /// * `origin_id`: The identifier of the node that sent the event.
    /// * `event`: The incoming event to be processed.
    ///   Returns:
    ///   * `Result<(), anyhow::Error>`: Returns Ok if the event was processed successfully, or an error if processing failed.
    fn process_incoming_event(&self, origin_id: Identifier, event: Event) -> anyhow::Result<()>;
}

/// Network trait defines the interface for a network service that can send and receive events.
#[unimock::unimock(api=NetworkMock)]
pub trait Network: Send + Sync {
    /// Sends an event to the network.
    fn send_event(&self, origin_id: Identifier, event: Event) -> anyhow::Result<()>;

    /// Registers an event processor to handle incoming events.
    /// At any point in time, there can be only one processor registered.
    /// Registering a new processor is illegal if there is already a processor registered, and causes an error.
    fn register_processor(&self, processor: MessageProcessor) -> anyhow::Result<()>;

    /// Creates a shallow copy of this networking layer instance.
    ///
    /// Implementations should ensure that cloned instances share the same underlying data
    /// (e.g., using Arc for shared ownership). Changes made through one instance should be
    /// visible in all cloned instances. This is the standard cloning behavior for all
    /// Network implementations.
    fn clone_box(&self) -> Box<dyn Network>;
}

impl Clone for Box<dyn Network> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}
