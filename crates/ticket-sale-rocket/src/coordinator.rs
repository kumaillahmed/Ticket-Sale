use std::sync::{Arc, Mutex};
use std::thread;
use rand::Rng;
use uuid::Uuid;
use crate::{
    database::DatabaseHelper,
    server::{Server, ServerHelper},
    balancer::MessagesForBalancer, // Import MessagesForBalancer from balancer.rs
};
use flume::{Receiver, Sender};

/// Enum for messages that can be sent to the Coordinator.
#[derive(Debug)]
pub enum MessagesForCoordinator {
    GetDatabase(Sender<DatabaseHelper>),
    GetNumOfServers(Sender<u32>),
    Scale(u32),
    RemoveShuttingDownServer(Uuid),
    GetAllServersIds(Sender<Vec<Uuid>>),
    PickRandomServer(Sender<Option<ServerHelper>>),
    GetServer(Uuid, Sender<Option<ServerHelper>>),
    ForceShutdown,
    // You can add more messages if needed.
}

/// Struct representing the Coordinator.
pub struct Coordinator {
    reservation_timeout: u32,
    database_helper: DatabaseHelper,
    servers: Arc<Mutex<Vec<ServerHelper>>>,
    shutting_down_servers: Arc<Mutex<Vec<ServerHelper>>>,
    sender: Sender<MessagesForCoordinator>,
    receiver: Receiver<MessagesForCoordinator>,
    balancer_sender: Sender<MessagesForBalancer>, // Sender for Balancer notifications.
}

impl Coordinator {
    /// Constructor for the Coordinator.
    pub fn new(
        reservation_timeout: u32,
        database_helper: DatabaseHelper,
        initial_servers: u32,
        balancer_sender: Sender<MessagesForBalancer>, // New parameter.
    ) -> Self {
        let (sender, receiver) = flume::unbounded();
        let servers = Arc::new(Mutex::new(Vec::new()));
        let shutting_down_servers = Arc::new(Mutex::new(Vec::new()));

        // Create the initial servers and start them.
        for _ in 0..initial_servers {
            Self::create_server(
                &servers,
                reservation_timeout,
                database_helper.clone(),
                sender.clone(),
                balancer_sender.clone(), // Pass Balancer sender.
            );
        }

        Self {
            reservation_timeout,
            database_helper,
            servers,
            shutting_down_servers,
            sender,
            receiver,
            balancer_sender, // Initialize the Balancer sender.
        }
    }

    /// Start function to run the Coordinator in a separate thread.
    pub fn start(self) {
        // Clone Arc pointers to move into the thread.
        let servers = Arc::clone(&self.servers);
        let shutting_down_servers = Arc::clone(&self.shutting_down_servers);
        let database_helper = self.database_helper.clone();
        let sender = self.sender.clone();
        let receiver = self.receiver.clone();
        let reservation_timeout = self.reservation_timeout;
        let balancer_sender = self.balancer_sender.clone(); // Clone Balancer sender.

        thread::spawn(move || {
            loop {
                if let Ok(message) = receiver.recv() {
                    match message {
                        MessagesForCoordinator::GetDatabase(sender_db) => {
                            sender_db.send(database_helper.clone()).unwrap_or_else(|_| {});
                        }
                        MessagesForCoordinator::GetNumOfServers(sender_num) => {
                            let num_servers = {
                                let servers_lock = servers.lock().unwrap();
                                servers_lock.len() as u32
                            };
                            sender_num.send(num_servers).unwrap_or_else(|_| {});
                        }
                        MessagesForCoordinator::Scale(num_servers) => {
                            Self::scale_servers(
                                num_servers,
                                &servers,
                                &shutting_down_servers,
                                reservation_timeout,
                                database_helper.clone(),
                                sender.clone(),
                                balancer_sender.clone(), // Pass Balancer sender.
                            );
                        }
                        MessagesForCoordinator::RemoveShuttingDownServer(server_id) => {
                            Self::remove_shutting_down_server(
                                server_id,
                                &shutting_down_servers,
                            );
                        }
                        MessagesForCoordinator::GetAllServersIds(sender_ids) => {
                            let all_ids = Self::get_all_server_ids(&servers);
                            sender_ids.send(all_ids).unwrap_or_else(|_| {});
                        }
                        MessagesForCoordinator::PickRandomServer(sender_rand) => {
                            let random_server = Self::pick_random_server(&servers);
                            sender_rand.send(random_server).unwrap_or_else(|_| {});
                        }
                        MessagesForCoordinator::GetServer(server_id, sender_server) => {
                            let server = Self::get_server(server_id, &servers, &shutting_down_servers);
                            sender_server.send(server).unwrap_or_else(|_| {});
                        }
                        MessagesForCoordinator::ForceShutdown => {
                            Self::force_shutdown(
                                &servers,
                                &shutting_down_servers,
                                balancer_sender.clone(),
                            );
                            break; // Exit the loop after forcing shutdown.
                        }
                    }
                }
            }
        });
    }

    /// Get a CoordinatorHelper for interacting with the Coordinator.
    pub fn get_coordinator_helper(&self) -> CoordinatorHelper {
        CoordinatorHelper {
            sender: self.sender.clone(),
        }
    }

    /// Creates a new server and adds it to the active servers list.
    fn create_server(
        servers: &Arc<Mutex<Vec<ServerHelper>>>,
        reservation_timeout: u32,
        database_helper: DatabaseHelper,
        coordinator_sender: Sender<MessagesForCoordinator>,
        balancer_sender: Sender<MessagesForBalancer>, // Pass Balancer sender.
    ) {
        let coordinator_helper = CoordinatorHelper {
            sender: coordinator_sender.clone(),
        };
        let server = Server::new(
            reservation_timeout,
            database_helper.clone(),
            coordinator_helper,
        );
        let server_helper = server.get_server_helper();
        server.start();

        servers.lock().unwrap().push(server_helper);
    }

    /// Scales the number of active servers to the desired number.
    fn scale_servers(
        num_servers: u32,
        servers: &Arc<Mutex<Vec<ServerHelper>>>,
        shutting_down_servers: &Arc<Mutex<Vec<ServerHelper>>>,
        reservation_timeout: u32,
        database_helper: DatabaseHelper,
        coordinator_sender: Sender<MessagesForCoordinator>,
        balancer_sender: Sender<MessagesForBalancer>, // Pass Balancer sender.
    ) {
        let current_servers = {
            let servers_lock = servers.lock().unwrap();
            servers_lock.len() as u32
        };

        match num_servers.cmp(&current_servers) {
            std::cmp::Ordering::Equal => {}
            std::cmp::Ordering::Greater => {
                let to_add = num_servers - current_servers;
                for _ in 0..to_add {
                    Self::create_server(
                        servers,
                        reservation_timeout,
                        database_helper.clone(),
                        coordinator_sender.clone(),
                        balancer_sender.clone(), // Pass Balancer sender.
                    );
                }
            }
            std::cmp::Ordering::Less => {
                let to_remove = current_servers - num_servers;
                for _ in 0..to_remove {
                    // Remove the last server from the list.
                    let server = {
                        let mut servers_lock = servers.lock().unwrap();
                        servers_lock.pop()
                    };
                    if let Some(server) = server {
                        server.shutdown();
                        // Add the server to the shutting_down_servers list.
                        {
                            let mut shutting_down_lock = shutting_down_servers.lock().unwrap();
                            shutting_down_lock.push(server.clone());
                        }
                        // Notify Balancer about server shutdown.
                        balancer_sender
                            .send(MessagesForBalancer::ServerShutdown(server.id()))
                            .unwrap_or_else(|_| {});
                    }
                }
            }
        }
    }

    /// Removes a server from the shutting_down_servers list.
    fn remove_shutting_down_server(
        server_id: Uuid,
        shutting_down_servers: &Arc<Mutex<Vec<ServerHelper>>>,
    ) {
        let mut shutting_down_lock = shutting_down_servers.lock().unwrap();
        shutting_down_lock.retain(|s| s.id() != server_id);
    }

    /// Gets a list of all active server IDs.
    fn get_all_server_ids(servers: &Arc<Mutex<Vec<ServerHelper>>>) -> Vec<Uuid> {
        let servers_lock = servers.lock().unwrap();
        servers_lock.iter().map(|s| s.id()).collect()
    }

    /// Picks a random active server.
    fn pick_random_server(servers: &Arc<Mutex<Vec<ServerHelper>>>) -> Option<ServerHelper> {
        let servers_lock = servers.lock().unwrap();
        if servers_lock.is_empty() {
            return None;
        }
        let rand_index = rand::thread_rng().gen_range(0..servers_lock.len());
        Some(servers_lock[rand_index].clone())
    }

    /// Gets a server by its ID, searching both active and shutting-down servers.
    fn get_server(
        server_id: Uuid,
        servers: &Arc<Mutex<Vec<ServerHelper>>>,
        shutting_down_servers: &Arc<Mutex<Vec<ServerHelper>>>,
    ) -> Option<ServerHelper> {
        // First, look for the server in active servers
        let servers_lock = servers.lock().unwrap();
        if let Some(server) = servers_lock.iter().find(|s| s.id() == server_id) {
            return Some(server.clone());
        }
        drop(servers_lock); // Release the lock before locking the next one

        // Then, look for the server in shutting-down servers
        let shutting_down_lock = shutting_down_servers.lock().unwrap();
        if let Some(server) = shutting_down_lock.iter().find(|s| s.id() == server_id) {
            return Some(server.clone());
        }
        None
    }

    /// Forces a shutdown of all servers.
    fn force_shutdown(
        servers: &Arc<Mutex<Vec<ServerHelper>>>,
        shutting_down_servers: &Arc<Mutex<Vec<ServerHelper>>>,
        balancer_sender: Sender<MessagesForBalancer>,
    ) {
        // Shutdown active servers.
        for server in servers.lock().unwrap().iter() {
            server.force_shutdown();
            // Notify Balancer about server shutdown.
            balancer_sender
                .send(MessagesForBalancer::ServerShutdown(server.id()))
                .unwrap_or_else(|_| {});
        }

        // Shutdown shutting-down servers.
        for server in shutting_down_servers.lock().unwrap().iter() {
            server.force_shutdown();
            // Notify Balancer about server shutdown.
            balancer_sender
                .send(MessagesForBalancer::ServerShutdown(server.id()))
                .unwrap_or_else(|_| {});
        }

        // Clear server lists.
        servers.lock().unwrap().clear();
        shutting_down_servers.lock().unwrap().clear();
    }
}

/// Helper struct to interact with the Coordinator.
#[derive(Clone)]
pub struct CoordinatorHelper {
    sender: Sender<MessagesForCoordinator>, // Sender to communicate with the Coordinator.
}

impl CoordinatorHelper {
    /// Gets a DatabaseHelper from the Coordinator.
    pub fn get_database(&self) -> DatabaseHelper {
        let (sender, receiver) = flume::unbounded();
        self.sender
            .send(MessagesForCoordinator::GetDatabase(sender))
            .unwrap();
        receiver.recv().unwrap()
    }

    /// Gets the number of active servers from the Coordinator.h
    pub fn get_num_of_servers(&self) -> u32 {
        let (sender, receiver) = flume::unbounded();
        self.sender
            .send(MessagesForCoordinator::GetNumOfServers(sender))
            .unwrap();
        receiver.recv().unwrap()
    }

    /// Scales the number of servers to the desired number.
    pub fn scale(&self, num_servers: u32) {
        self.sender
            .send(MessagesForCoordinator::Scale(num_servers))
            .unwrap();
    }

    /// Picks a random server helper from active servers.
    pub fn pick_random_server(&self) -> Option<ServerHelper> {
        let (sender, receiver) = flume::unbounded();
        self.sender
            .send(MessagesForCoordinator::PickRandomServer(sender))
            .unwrap();
        receiver.recv().unwrap()
    }

    /// Gets all active server IDs.
    pub fn get_all_server_ids(&self) -> Vec<Uuid> {
        let (sender, receiver) = flume::unbounded();
        self.sender
            .send(MessagesForCoordinator::GetAllServersIds(sender))
            .unwrap();
        receiver.recv().unwrap()
    }

    /// Gets a server by its ID.
    pub fn get_server(&self, server_id: Uuid) -> Option<ServerHelper> {
        let (sender, receiver) = flume::unbounded();
        self.sender
            .send(MessagesForCoordinator::GetServer(server_id, sender))
            .unwrap();
        receiver.recv().unwrap()
    }

    /// Instructs the Coordinator to force shutdown all servers.
    pub fn force_shutdown(&self) {
        self.sender
            .send(MessagesForCoordinator::ForceShutdown)
            .unwrap();
    }

    /// Sends a message to remove a shutting down server.
    pub fn remove_shutting_down_server(&self, server_id: Uuid) {
        self.sender
            .send(MessagesForCoordinator::RemoveShuttingDownServer(server_id))
            .unwrap();
    }
}