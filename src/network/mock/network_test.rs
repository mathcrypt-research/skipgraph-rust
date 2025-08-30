use crate::core::testutil::fixtures::random_identifier;
use crate::network::mock::hub::NetworkHub;
use crate::network::Event::TestMessage;
use crate::network::{MessageProcessor, EventProcessorCore, Network, Event};
use std::collections::HashSet;
use std::sync::{Arc, Barrier, RwLock};
use std::thread;
use crate::core::Identifier;

struct MockEventProcessor {
    inner: Arc<RwLock<MockEventProcessorInner>>,
}

struct MockEventProcessorInner {
    seen: HashSet<String>,
}

impl MockEventProcessor {
    fn new() -> Self {
        MockEventProcessor {
            inner: Arc::new(RwLock::new(MockEventProcessorInner {
                seen: HashSet::new(),
            })),
        }
    }

    fn has_seen(&self, content: &str) -> bool {
        self.inner.read().unwrap().seen.contains(content)
    }
}

impl Clone for MockEventProcessor {
    fn clone(&self) -> Self {
        MockEventProcessor {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl EventProcessorCore for MockEventProcessor {
    fn process_incoming_event(&self, _origin_id: Identifier, event: Event) -> anyhow::Result<()> {
        match event {
            TestMessage(content) => {
                // TODO: make this a hash table and track content with origin_id
                self.inner.write().unwrap().seen.insert(content);
                Ok(())
            }
            _ => Err(anyhow::anyhow!("MockEventProcessor only handles TestMessage payloads")),
        }
    }
}

/// This test verifies that `MockEventProcessor` correctly processes and tracks incoming events routed through a mock network.
#[test]
fn test_mock_event_processor() {
    let hub = NetworkHub::new();
    let target_id = random_identifier();
    let mock_network = NetworkHub::new_mock_network(hub.clone(), target_id).unwrap();
    let core_processor = MockEventProcessor::new();
    let processor = MessageProcessor::new(Box::new(core_processor.clone()));
    let event = TestMessage("Hello, World!".to_string());


    assert!(!core_processor.has_seen("Hello, World!"));
    
    assert!(mock_network
        .register_processor(processor)
        .is_ok());
    let origin_id = random_identifier();
    assert!(hub.route_event(origin_id, target_id, event).is_ok());
    
    assert!(core_processor.has_seen("Hello, World!"));
}

/// This test ensures correct routing and processing of events between mock networks through the `NetworkHub`.
#[test]
fn test_hub_route_event() {
    let hub = NetworkHub::new();

    let id_1 = random_identifier();
    let mock_net_1 = NetworkHub::new_mock_network(hub.clone(), id_1).unwrap();
    let core_proc_1 = MockEventProcessor::new();
    let msg_proc_1 = MessageProcessor::new(Box::new(core_proc_1.clone()));
    mock_net_1
        .register_processor(msg_proc_1)
        .expect("failed to register event processor");

    let id_2 = random_identifier();
    let mock_net_2 = NetworkHub::new_mock_network(hub, id_2).unwrap();

    let event = TestMessage("Test message".to_string());

    assert!(!core_proc_1.has_seen("Test message"));
    
    assert!(mock_net_2.send_event(id_1, event).is_ok());
    
    assert!(core_proc_1.has_seen("Test message"));
}

/// This test verifies that cloning a NetworkHub results in a shallow copy where cloned instances share the same underlying data.
#[test]
fn test_network_hub_shallow_clone() {
    let hub = NetworkHub::new();
    let hub_clone = hub.clone();
    
    let target_id = random_identifier();
    
    // Create a mock network through the original hub
    let mock_network = NetworkHub::new_mock_network(hub.clone(), target_id).unwrap();
    
    // Create an event to route through the cloned hub
    let event = TestMessage("Shallow clone test".to_string());
    
    // Register a processor on the mock network
    let core_processor = MockEventProcessor::new();
    let processor = MessageProcessor::new(Box::new(core_processor.clone()));
    mock_network
        .register_processor(processor)
        .expect("failed to register event processor");
    
    // Verify the event hasn't been seen yet
    assert!(!core_processor.has_seen("Shallow clone test"));
    
    // Route event through the CLONED hub - this should work because it shares the same underlying data
    let origin_id = random_identifier();
    assert!(hub_clone.route_event(origin_id, target_id, event).is_ok());
    
    // Verify the event was processed - proving the clone shares the same networks map
    assert!(core_processor.has_seen("Shallow clone test"));
}

/// This test sends 10 events concurrently from mock_net_2 to id_1 and verifies that all events are processed.
#[test]
fn test_concurrent_event_sending() {
    let hub = NetworkHub::new();

    let id_1 = random_identifier();
    let mock_net_1 = NetworkHub::new_mock_network(hub.clone(), id_1).unwrap();
    let core_proc_1 = MockEventProcessor::new();
    let msg_proc_1 = MessageProcessor::new(Box::new(core_proc_1.clone()));
    mock_net_1
        .register_processor(msg_proc_1)
        .expect("failed to register event processor");

    let id_2 = random_identifier();
    let mock_net_2 = NetworkHub::new_mock_network(hub, id_2).unwrap();

    // Create 10 different event contents
    let event_contents: Vec<String> =
        (0..10).map(|i| format!("Concurrent message {i}")).collect();

    // Set up a barrier to synchronize all threads
    let barrier = Arc::new(Barrier::new(10));
    let mut handles = vec![];

    // Spawn 10 threads, each sending a different event
    for content in event_contents.iter() {
        let content = content.clone();
        let barrier_clone = barrier.clone();
        let mock_net_2_clone = mock_net_2.clone();

        let handle = thread::spawn(move || {
            let event = TestMessage(content);

            // Wait for all threads to reach this point
            barrier_clone.wait();

            // Send the event
            mock_net_2_clone.send_event(id_1, event).unwrap();
        });

        handles.push(handle);
    }

    // Wait for all threads to complete
    for handle in handles {
        handle.join().unwrap();
    }

    // Verify that all events were received
    for content in event_contents {
        assert!(
            core_proc_1.has_seen(&content),
            "Event '{content}' was not received"
        );
        println!("Event '{content}' was successfully processed");
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
    let core_processor = MockEventProcessor::new();
    let processor = MessageProcessor::new(Box::new(core_processor.clone()));
    
    assert!(mock_network.register_processor(processor).is_ok());
    
    // Create an event to test with
    let event = TestMessage("Shared processor test".to_string());
    
    // Verify the event hasn't been seen yet
    assert!(!core_processor.has_seen("Shared processor test"));
    
    // Send event through the CLONED network - should work because processor is shared
    let origin_id = random_identifier();
    assert!(mock_network_clone.incoming_event(origin_id, event).is_ok());
    
    // Verify the event was processed through the shared processor
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
    let core_processor = MockEventProcessor::new();
    let processor = MessageProcessor::new(Box::new(core_processor.clone()));
    
    assert!(mock_network_clone.register_processor(processor).is_ok());
    
    // Create an event to test with
    let event = TestMessage("Clone to original test".to_string());
    
    // Verify the event hasn't been seen yet
    assert!(!core_processor.has_seen("Clone to original test"));
    
    // Send event through the ORIGINAL network - should work because processor is shared
    let origin_id = random_identifier();
    assert!(mock_network.incoming_event(origin_id, event).is_ok());
    
    // Verify the event was processed through the shared processor
    assert!(core_processor.has_seen("Clone to original test"));
}

/// This test verifies that processor cloning itself works correctly by ensuring
/// multiple processor instances share the same underlying state.
#[test]
fn test_event_processor_clone_functionality() {
    let core_processor = MockEventProcessor::new();
    let processor1 = MessageProcessor::new(Box::new(core_processor.clone()));
    let processor2 = processor1.clone();

    
    // Create test events
    let event1 = TestMessage("Processor clone test 1".to_string());
    
    let event2 =  TestMessage("Processor clone test 2".to_string());

    
    // Verify events haven't been seen yet
    assert!(!core_processor.has_seen("Processor clone test 1"));
    assert!(!core_processor.has_seen("Processor clone test 2"));
    
    // Process first event with first processor
    let  origin_id = random_identifier();
    assert!(processor1.process_incoming_event(origin_id, event1).is_ok());
    assert!(core_processor.has_seen("Processor clone test 1"));
    
    // Process second event with cloned processor
    assert!(processor2.process_incoming_event(origin_id, event2).is_ok());
    assert!(core_processor.has_seen("Processor clone test 2"));
    
    // Both events should be visible from the shared state
    assert!(core_processor.has_seen("Processor clone test 1"));
    assert!(core_processor.has_seen("Processor clone test 2"));
}
