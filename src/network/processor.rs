use anyhow::anyhow;
use std::sync::{Arc, RwLock};
use crate::network::{Message, MessageProcessorCore};

/// A thread-safe wrapper that enforces internal thread-safety for message processors.
/// This type guarantees that all message processing is properly synchronized.
#[derive(Clone)]
pub struct MessageProcessor {
    core: Arc<RwLock<Box<dyn MessageProcessorCore>>>,
}

impl MessageProcessor {
    /// Creates a new thread-safe message processor from a core implementation.
    pub fn new(core: Box<dyn MessageProcessorCore>) -> Self {
        Self {
            core: Arc::new(RwLock::new(core)),
        }
    }

    /// Process an incoming message with guaranteed thread-safety.
    pub fn process_incoming_message(&self, message: Message) -> anyhow::Result<()> {
        let core = self.core.read()
            .map_err(|_| anyhow!("Failed to acquire read lock on message processor"))?;
        core.process_incoming_message(message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use crate::core::Identifier;
    use crate::network::Payload;
    
    // A mock implementation of MessageProcessorCore that counts the number of processed messages.
    struct MockMessageProcessorCore {
        counter: Arc<AtomicUsize>,
    }

    impl MockMessageProcessorCore {
        fn new() -> Self {
            Self {
                counter: Arc::new(AtomicUsize::new(0)),
            }
        }

        fn get_counter(&self) -> Arc<AtomicUsize> {
            Arc::clone(&self.counter)
        }
    }

    impl MessageProcessorCore for MockMessageProcessorCore {
        fn process_incoming_message(&self, _message: Message) -> anyhow::Result<()> {
            self.counter.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    /// This test verifies that `MessageProcessor` correctly supports shallow cloning,
    /// i.e., cloned instances share the same underlying core processor.
    #[test]
    fn test_message_processor_shallow_cloning() {
        let mock_core = MockMessageProcessorCore::new();
        let counter_ref = mock_core.get_counter();
        let processor = MessageProcessor::new(Box::new(mock_core));
        let processor_clone = processor.clone();

        let test_message = Message {
            payload: Payload::TestMessage("test".to_string()),
            target_node_id: Identifier::from_bytes(&[0; 32]).unwrap(),
        };

        assert_eq!(counter_ref.load(Ordering::SeqCst), 0);

        processor.process_incoming_message(test_message).unwrap();
        assert_eq!(counter_ref.load(Ordering::SeqCst), 1);

        let test_message2 = Message {
            payload: Payload::TestMessage("test2".to_string()),
            target_node_id: Identifier::from_bytes(&[1; 32]).unwrap(),
        };

        processor_clone.process_incoming_message(test_message2).unwrap();
        assert_eq!(counter_ref.load(Ordering::SeqCst), 2);
    }
}