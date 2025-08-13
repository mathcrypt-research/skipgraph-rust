use crate::core::testutil::fixtures::random_identifier;
use crate::network::mock::hub::NetworkHub;
use crate::network::Payload::TestMessage;
use crate::network::{Message, MessageProcessor, Network};
use std::collections::HashSet;
use std::sync::{Arc, Barrier, Mutex};
use std::thread;

struct MockMessageProcessor {
    seen: HashSet<String>,
}

impl MockMessageProcessor {
    fn new() -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(MockMessageProcessor {
            seen: HashSet::new(),
        }))
    }

    fn has_seen(&self, content: &str) -> bool {
        self.seen.contains(content)
    }
}

impl MessageProcessor for MockMessageProcessor {
    fn process_incoming_message(&mut self, message: Message) -> anyhow::Result<()> {
        match message.payload {
            TestMessage(content) => {
                self.seen.insert(content);
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
    let processor = MockMessageProcessor::new();
    let message = Message {
        payload: TestMessage("Hello, World!".to_string()),
        target_node_id: identifier,
    };

    {
        let proc_guard = processor.lock().unwrap();
        assert!(!proc_guard.has_seen("Hello, World!"));
    }
    {
        let mut network_guard = mock_network.lock().unwrap();
        assert!(network_guard
            .register_processor(Box::new(processor.clone()))
            .is_ok());
    }
    {
        let hub_guard = hub.lock().unwrap();
        assert!(hub_guard.route_message(message).is_ok());
    }
    {
        let proc_guard = processor.lock().unwrap();
        assert!(proc_guard.has_seen("Hello, World!"));
    }
}

/// This test ensures correct routing and processing of messages between mock networks through the `NetworkHub`.
#[test]
fn test_hub_route_message() {
    let hub = NetworkHub::new();

    let id_1 = random_identifier();
    let mock_net_1 = NetworkHub::new_mock_network(hub.clone(), id_1).unwrap();
    let msg_proc_1 = MockMessageProcessor::new();
    {
        let mut net_guard = mock_net_1.lock().unwrap();
        net_guard
            .register_processor(Box::new(msg_proc_1.clone()))
            .expect("Failed to register message processor");
    }

    let id_2 = random_identifier();
    let mock_net_2 = NetworkHub::new_mock_network(hub, id_2).unwrap();

    let message = Message {
        payload: TestMessage("Test message".to_string()),
        target_node_id: id_1,
    };

    {
        let proc_guard = msg_proc_1.lock().unwrap();
        assert!(!proc_guard.has_seen("Test message"));
    }
    {
        let net_guard = mock_net_2.lock().unwrap();
        assert!(net_guard.send_message(message).is_ok());
    }
    {
        let proc_guard = msg_proc_1.lock().unwrap();
        assert!(proc_guard.has_seen("Test message"));
    }
}

/// This test sends 10 messages concurrently from mock_net_2 to id_1 and verifies that all messages are processed.
#[test]
fn test_concurrent_message_sending() {
    let hub = NetworkHub::new();

    let id_1 = random_identifier();
    let mock_net_1 = NetworkHub::new_mock_network(hub.clone(), id_1).unwrap();
    let msg_proc_1 = MockMessageProcessor::new();
    {
        let mut net_guard = mock_net_1.lock().unwrap();
        net_guard
            .register_processor(Box::new(msg_proc_1.clone()))
            .expect("Failed to register message processor");
    }

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
            let net_guard = mock_net_2_clone.lock().unwrap();
            net_guard.send_message(message).unwrap();
        });

        handles.push(handle);
    }

    // Wait for all threads to complete
    for handle in handles {
        handle.join().unwrap();
    }

    // Verify that all messages were received
    let processor = msg_proc_1.lock().unwrap();
    for content in message_contents {
        assert!(
            processor.has_seen(&content),
            "Message '{content}' was not received"
        );
        println!("Message '{content}' was successfully processed");
    }
}
