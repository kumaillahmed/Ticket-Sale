use ticket_sale_core::{Request, RequestHandler, RequestKind};
use crate::coordinator::CoordinatorHelper;
use uuid::Uuid;
use flume::Receiver;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Enum for messages that the Balancer can receive.
#[derive(Debug)]
pub enum MessagesForBalancer {
    ServerShutdown(Uuid), // Notify Balancer that a server has shut down.
    // Add other Balancer-specific messages if needed.
}

/// Struct representing the Balancer component.
/// The Balancer distributes incoming requests to the appropriate servers.
pub struct Balancer {
    coordinator: CoordinatorHelper, // Helper to interact with the Coordinator for server management.
    estimator_shutdown_sender: flume::Sender<()>, // Sender used to signal the shutdown of the Estimator.
    shutdown_receiver: Receiver<MessagesForBalancer>, // Receiver for shutdown notifications.
    customer_to_server: Arc<Mutex<HashMap<Uuid, Uuid>>>, // Mapping of customer_id to server_id
}

impl Balancer {
    /// Constructor for the Balancer, initializing the CoordinatorHelper and shutdown receiver.
    pub fn new(
        coordinator: CoordinatorHelper,
        estimator_shutdown_sender: flume::Sender<()>,
        shutdown_receiver: Receiver<MessagesForBalancer>, // Initialize receiver.
    ) -> Self {
        Self {
            coordinator,
            estimator_shutdown_sender,
            shutdown_receiver,
            customer_to_server: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

// Implementing the RequestHandler trait for Balancer.
impl RequestHandler for Balancer {
    /// Handle function to process incoming requests.
    fn handle(&self, mut rq: Request) {
        let client_id = rq.customer_id();

        // In Balancer's handle method
        while let Ok(message) = self.shutdown_receiver.try_recv() {
            match message {
                MessagesForBalancer::ServerShutdown(server_id) => {
                    // Acquire the lock on the customer_to_server mapping
                    let mut map = self.customer_to_server.lock().unwrap();
                    // Retain only those mappings where the server_id is not the terminating server
                    map.retain(|_, &mut s_id| s_id != server_id);
                    // Optionally, remove the server from an active servers list if maintained
                    // self.active_servers.lock().unwrap().remove(&server_id);
                }
                // Handle other Balancer-specific messages if any.
            }
        }


        match rq.kind() {
            RequestKind::NumAvailableTickets => {
                // Aggregate available tickets from all active servers
                let server_ids = self.coordinator.get_all_server_ids();
                let mut total_available = 0;

                for server_id in &server_ids {
                    if let Some(server) = self.coordinator.get_server(*server_id) {
                        total_available += server.handle_estimator();
                    }
                }

                // Get available tickets from the Database
                let database_available = self.coordinator.get_database().get_num_available();
                total_available += database_available;

                rq.respond_with_int(total_available);
                return;
            }
            RequestKind::SetNumServers => {
                if let Some(num_servers) = rq.read_u32() {
                    self.coordinator.scale(num_servers);
                    rq.respond_with_int(num_servers);
                } else {
                    rq.respond_with_err("No number provided in SetNumServers request.");
                }
                return;
            }
            RequestKind::GetNumServers => {
                let num_servers = self.coordinator.get_num_of_servers();
                rq.respond_with_int(num_servers);
                return;
            }
            RequestKind::GetServers => {
                let server_ids = self.coordinator.get_all_server_ids();
                rq.respond_with_server_list(&server_ids);
                return;
            }
            _ => {}
        }

        // Determine the server to handle this request
        let server_id_option = {
            let mut map = self.customer_to_server.lock().unwrap();
            if let Some(&s_id) = map.get(&client_id) {
                Some(s_id)
            } else {
                // Assign a new server
                match self.coordinator.pick_random_server() {
                    Some(server) => {
                        let s_id = server.id();
                        map.insert(client_id, s_id);
                        Some(s_id)
                    }
                    None => {
                        None
                    }
                }
            }
        };

        match server_id_option {
            Some(server_id) => {
                // Retrieve the server helper
                match self.coordinator.get_server(server_id) {
                    Some(server) => {
                        rq.set_server_id(server_id);
                        server.handle_request(rq);
                    }
                    None => {
                        // The server might have been shutdown. Remove the mapping and reassign
                        {
                            let mut map = self.customer_to_server.lock().unwrap();
                            map.remove(&client_id);
                        }
        
                        // Attempt to pick a new active server
                        if let Some(new_server) = self.coordinator.pick_random_server() {
                            let new_server_id = new_server.id();
                            {
                                let mut map = self.customer_to_server.lock().unwrap();
                                map.insert(client_id, new_server_id);
                            }
                            rq.set_server_id(new_server_id);
                            new_server.handle_request(rq);
                        } else {
                            // No active servers available
                            rq.respond_with_err("No active servers available to handle the request.");
                        }
                    }
                }
            }
            None => {
                // No server assigned yet; assign a new active server
                if let Some(new_server) = self.coordinator.pick_random_server() {
                    let new_server_id = new_server.id();
                    {
                        let mut map = self.customer_to_server.lock().unwrap();
                        map.insert(client_id, new_server_id);
                    }
                    rq.set_server_id(new_server_id);
                    new_server.handle_request(rq);
                } else {
                    // No active servers available
                    rq.respond_with_err("No active servers available to handle the request.");
                }
            }
        }        
    }

    /// Shutdown function to gracefully shut down the balancer and associated components.
    fn shutdown(self) {
        self.coordinator.force_shutdown(); // Instruct the Coordinator to force a shutdown of all servers.
        self.estimator_shutdown_sender.send(()).unwrap(); // Send a signal to shut down the Estimator.
    }
}