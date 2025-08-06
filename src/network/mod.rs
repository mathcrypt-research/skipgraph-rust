pub mod mock;

use crate::core::Identifier;
use std::cell::RefCell;
use std::rc::Rc;

/// Payload enum defines the semantics of the message payload that can be sent over the network.
#[derive(Debug)]
pub enum Payload {
    TestMessage(String), // A payload for testing purposes, it is a simple string message, and is not used in production.
}

/// Message struct represents a message that can be sent over the network.
pub struct Message {
    pub payload: Payload,
    pub target_node_id: Identifier,
}

/// MessageProcessor trait defines the entity that processes the incoming network messages at this node.
pub trait MessageProcessor {
    fn process_incoming_message(&mut self, message: Message) -> anyhow::Result<()>;
}

/// Network trait defines the interface for a network service that can send and receive messages.
pub trait Network {
    /// Sends a message to the network.
    fn send_message(&self, message: Message) -> anyhow::Result<()>;

    /// Registers a message processor to handle incoming messages.
    /// At any point in time, there can be only one processor registered.
    /// Registering a new processor is illegal if there is already a processor registered, and causes an error.
    fn register_processor(
        &mut self,
        processor: Box<Rc<RefCell<dyn MessageProcessor>>>,
    ) -> anyhow::Result<()>;
}
