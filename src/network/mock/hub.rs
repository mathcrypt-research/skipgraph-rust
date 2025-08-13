use crate::core::Identifier;
use crate::network::mock::network::MockNetwork;
use crate::network::Message;
use anyhow::{anyhow, Context};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

/// NetworkHub is a central hub that manages multiple mock networks.
/// It allows for the creation of new mock networks and routing messages between them.
/// Messages are routed completely through the hub in an in-memory fashion, simulating a network environment without actual network communication.
pub struct NetworkHub {
    networks: RwLock<HashMap<Identifier, Arc<Mutex<MockNetwork>>>>,
}

impl NetworkHub {
    pub fn new() -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(NetworkHub {
            networks: RwLock::new(HashMap::new()),
        }))
    }

    /// Creates a new mock network with the given identifier and registers it in the hub.
    pub fn new_mock_network(
        hub: Arc<Mutex<Self>>,
        identifier: Identifier,
    ) -> anyhow::Result<Arc<Mutex<MockNetwork>>> {
        let inner_hub = hub
            .lock()
            .map_err(|_| anyhow!("Failed to acquire lock on hub"))?;
        let mut inner_networks = inner_hub
            .networks
            .write()
            .map_err(|_| anyhow!("Failed to acquire write lock on network hub"))?;
        if inner_networks.contains_key(&identifier) {
            return Err(anyhow::anyhow!(
                "Network with identifier {} already exists",
                identifier
            ));
        }
        let mock_network = Arc::new(Mutex::new(MockNetwork::new(hub.clone())));
        inner_networks.insert(identifier, mock_network.clone());
        Ok(mock_network)
    }

    /// Routes a message to the appropriate mock network based on the target node identifier.
    pub fn route_message(&self, message: Message) -> anyhow::Result<()> {
        let inner_networks = self
            .networks
            .read()
            .map_err(|_| anyhow!("Failed to acquire read lock on network hub"))?;
        if let Some(network) = inner_networks.get(&message.target_node_id) {
            let network_guard = network
                .lock()
                .map_err(|_| anyhow!("Failed to acquire lock on network"))?;
            network_guard
                .incoming_message(message)
                .context("Failed to send message through network")?;
            Ok(())
        } else {
            Err(anyhow!(
                "Network with identifier {} not found",
                message.target_node_id
            ))
        }
    }
}
