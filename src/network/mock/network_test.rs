use crate::core::testutil::fixtures::random_identifier;
use crate::network::mock::hub::NetworkHub;
use crate::network::Payload::TestMessage;
use crate::network::{Message, MessageProcessor, MessageProcessorCore, Network};
use std::collections::HashSet;
use std::sync::{Arc, Barrier, RwLock};
use std::thread;

struct MockMessageProcessor {
    inner: Arc<RwLock<MockMessageProcessorInner>>,
}

struct MockMessageProcessorInner {
    seen: HashSet<String>,
}

impl MockMessageProcessor {
    fn new() -> Self {
        MockMessageProcessor {
            inner: Arc::new(RwLock::new(MockMessageProcessorInner {
                seen: HashSet::new(),
            })),
        }
    }

    fn has_seen(&self, content: &str) -> bool {
        self.inner.read().unwrap().seen.contains(content)
    }
}

impl Clone for MockMessageProcessor {
    fn clone(&self) -> Self {
        MockMessageProcessor {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl MessageProcessorCore for MockMessageProcessor {
    fn process_incoming_message(&self, message: Message) -> anyhow::Result<()> {
        match message.payload {
            TestMessage(content) => {
                self.inner.write().unwrap().seen.insert(content);
                Ok(())
            }
        }
    }
}

/// This test verifies that `MockMessageProcessor` correctly processes and tracks incoming messages routed through a mock network.
#[test]
fn test_mock_message_processor() {
    let hub = NetworkHub::new();
    let identifier = random_identifier();
    let mock_network = NetworkHub::new_mock_network(hub.clone(), identifier).unwrap();
    let core_processor = MockMessageProcessor::new();
    let processor = MessageProcessor::new(Box::new(core_processor.clone()));
    let message = Message {
        payload: TestMessage("Hello, World!".to_string()),
        target_node_id: identifier,
    };

    assert!(!core_processor.has_seen("Hello, World!"));
    
    assert!(mock_network
        .register_processor(processor)
        .is_ok());
    
    assert!(hub.route_message(message).is_ok());
    
    assert!(core_processor.has_seen("Hello, World!"));
}

/// This test ensures correct routing and processing of messages between mock networks through the `NetworkHub`.
#[test]
fn test_hub_route_message() {
    let hub = NetworkHub::new();

    let id_1 = random_identifier();
    let mock_net_1 = NetworkHub::new_mock_network(hub.clone(), id_1).unwrap();
    let core_proc_1 = MockMessageProcessor::new();
    let msg_proc_1 = MessageProcessor::new(Box::new(core_proc_1.clone()));
    mock_net_1
        .register_processor(msg_proc_1)
        .expect("Failed to register message processor");

    let id_2 = random_identifier();
    let mock_net_2 = NetworkHub::new_mock_network(hub, id_2).unwrap();

    let message = Message {
        payload: TestMessage("Test message".to_string()),
        target_node_id: id_1,
    };

    assert!(!core_proc_1.has_seen("Test message"));
    
    assert!(mock_net_2.send_message(message).is_ok());
    
    assert!(core_proc_1.has_seen("Test message"));
}

/// This test verifies that cloning a NetworkHub results in a shallow copy where cloned instances share the same underlying data.
#[test]
fn test_network_hub_shallow_clone() {
    let hub = NetworkHub::new();
    let hub_clone = hub.clone();
    
    let identifier = random_identifier();
    
    // Create a mock network through the original hub
    let mock_network = NetworkHub::new_mock_network(hub.clone(), identifier).unwrap();
    
    // Create a message to route through the cloned hub
    let message = Message {
        payload: TestMessage("Shallow clone test".to_string()),
        target_node_id: identifier,
    };
    
    // Register a processor on the mock network
    let core_processor = MockMessageProcessor::new();
    let processor = MessageProcessor::new(Box::new(core_processor.clone()));
    mock_network
        .register_processor(processor)
        .expect("Failed to register message processor");
    
    // Verify the message hasn't been seen yet
    assert!(!core_processor.has_seen("Shallow clone test"));
    
    // Route message through the CLONED hub - this should work because it shares the same underlying data
    assert!(hub_clone.route_message(message).is_ok());
    
    // Verify the message was processed - proving the clone shares the same networks map
    assert!(core_processor.has_seen("Shallow clone test"));
}

/// This test sends 10 messages concurrently from mock_net_2 to id_1 and verifies that all messages are processed.
#[test]
fn test_concurrent_message_sending() {
    let hub = NetworkHub::new();

    let id_1 = random_identifier();
    let mock_net_1 = NetworkHub::new_mock_network(hub.clone(), id_1).unwrap();
    let core_proc_1 = MockMessageProcessor::new();
    let msg_proc_1 = MessageProcessor::new(Box::new(core_proc_1.clone()));
    mock_net_1
        .register_processor(msg_proc_1)
        .expect("Failed to register message processor");

    let id_2 = random_identifier();
    let mock_net_2 = NetworkHub::new_mock_network(hub, id_2).unwrap();

    // Create 10 different message contents
    let message_contents: Vec<String> =
        (0..10).map(|i| format!("Concurrent message {i}")).collect();

    // Set up a barrier to synchronize all threads
    let barrier = Arc::new(Barrier::new(10));
    let mut handles = vec![];

    // Spawn 10 threads, each sending a different message
    for content in message_contents.iter() {
        let content = content.clone();
        let barrier_clone = barrier.clone();
        let mock_net_2_clone = mock_net_2.clone();
        let id_1_copy = id_1;

        let handle = thread::spawn(move || {
            let message = Message {
                payload: TestMessage(content),
                target_node_id: id_1_copy,
            };

            // Wait for all threads to reach this point
            barrier_clone.wait();

            // Send the message
            mock_net_2_clone.send_message(message).unwrap();
        });

        handles.push(handle);
    }

    // Wait for all threads to complete
    for handle in handles {
        handle.join().unwrap();
    }

    // Verify that all messages were received
    for content in message_contents {
        assert!(
            core_proc_1.has_seen(&content),
            "Message '{content}' was not received"
        );
        println!("Message '{content}' was successfully processed");
    }
}

/// This test verifies that MockNetwork clones properly share processor state.
/// When a processor is registered on one instance, it should be accessible from cloned instances.
#[test]
fn test_mock_network_processor_sharing_between_clones() {
    let hub = NetworkHub::new();
    let identifier = random_identifier();
    let mock_network = NetworkHub::new_mock_network(hub.clone(), identifier).unwrap();
    
    // Clone the network before registering a processor
    let mock_network_clone = mock_network.clone();
    
    // Register a processor on the original network
    let core_processor = MockMessageProcessor::new();
    let processor = MessageProcessor::new(Box::new(core_processor.clone()));
    
    assert!(mock_network.register_processor(processor).is_ok());
    
    // Create a message to test with
    let message = Message {
        payload: TestMessage("Shared processor test".to_string()),
        target_node_id: identifier,
    };
    
    // Verify the message hasn't been seen yet
    assert!(!core_processor.has_seen("Shared processor test"));
    
    // Send message through the CLONED network - should work because processor is shared
    assert!(mock_network_clone.incoming_message(message).is_ok());
    
    // Verify the message was processed through the shared processor
    assert!(core_processor.has_seen("Shared processor test"));
}

/// This test verifies that when a processor is registered on a clone, it's accessible from the original.
#[test]
fn test_mock_network_processor_sharing_clone_to_original() {
    let hub = NetworkHub::new();
    let identifier = random_identifier();
    let mock_network = NetworkHub::new_mock_network(hub.clone(), identifier).unwrap();
    
    // Clone the network
    let mock_network_clone = mock_network.clone();
    
    // Register a processor on the CLONED network
    let core_processor = MockMessageProcessor::new();
    let processor = MessageProcessor::new(Box::new(core_processor.clone()));
    
    assert!(mock_network_clone.register_processor(processor).is_ok());
    
    // Create a message to test with
    let message = Message {
        payload: TestMessage("Clone to original test".to_string()),
        target_node_id: identifier,
    };
    
    // Verify the message hasn't been seen yet
    assert!(!core_processor.has_seen("Clone to original test"));
    
    // Send message through the ORIGINAL network - should work because processor is shared
    assert!(mock_network.incoming_message(message).is_ok());
    
    // Verify the message was processed through the shared processor
    assert!(core_processor.has_seen("Clone to original test"));
}

/// This test verifies that processor cloning itself works correctly by ensuring
/// multiple processor instances share the same underlying state.
#[test]
fn test_message_processor_clone_functionality() {
    let core_processor = MockMessageProcessor::new();
    let processor1 = MessageProcessor::new(Box::new(core_processor.clone()));
    let processor2 = processor1.clone();
    
    let identifier = random_identifier();
    
    // Create test messages
    let message1 = Message {
        payload: TestMessage("Processor clone test 1".to_string()),
        target_node_id: identifier,
    };
    
    let message2 = Message {
        payload: TestMessage("Processor clone test 2".to_string()),
        target_node_id: identifier,
    };
    
    // Verify messages haven't been seen yet
    assert!(!core_processor.has_seen("Processor clone test 1"));
    assert!(!core_processor.has_seen("Processor clone test 2"));
    
    // Process first message with first processor
    assert!(processor1.process_incoming_message(message1).is_ok());
    assert!(core_processor.has_seen("Processor clone test 1"));
    
    // Process second message with cloned processor
    assert!(processor2.process_incoming_message(message2).is_ok());
    assert!(core_processor.has_seen("Processor clone test 2"));
    
    // Both messages should be visible from the shared state
    assert!(core_processor.has_seen("Processor clone test 1"));
    assert!(core_processor.has_seen("Processor clone test 2"));
}
