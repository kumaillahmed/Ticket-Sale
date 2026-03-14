use std::{
    collections::{hash_map::Entry, HashMap},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
use ticket_sale_core::{Request, RequestKind};
use uuid::Uuid;
use crate::{
    constants::TICKET_BATCH_SIZE,
    coordinator::CoordinatorHelper,
    database::DatabaseHelper,
};
use flume::{Receiver, Sender};

/// Enum for high-priority system messages.
#[derive(Debug)]
pub enum SystemMessagesForServer {
    Id(Sender<Uuid>),
    HandleEstimator(Sender<u32>),
    Shutdown,
    CanShutdown(Sender<bool>),
    ForceShutdown,
}

/// Enum for low-priority client requests.
#[derive(Debug)]
pub enum ClientMessagesForServer {
    HandleRequest(Request),
}

/// Struct representing a ticket-selling server.
#[derive(Clone)]
pub struct Server {
    id: Uuid, // Unique ID for the server.
    available_tickets: Arc<Mutex<Vec<u32>>>, // Tickets available for sale.
    reservations: Arc<Mutex<HashMap<Uuid, Reservation>>>, // Map of customer reservations.
    reservation_timeout: u32, // Timeout for reservations in seconds.
    database_helper: DatabaseHelper, // Helper to interact with the database.
    coordinator: CoordinatorHelper, // Helper to interact with the coordinator.
    is_shutting_down: Arc<Mutex<bool>>, // Flag indicating if the server is shutting down.
    fully_shut_down: Arc<Mutex<bool>>, // Flag indicating shutdown completion.

    // Fields to store the senders for system and client messages.
    system_sender: Sender<SystemMessagesForServer>,
    client_sender: Sender<ClientMessagesForServer>,
    system_receiver: Receiver<SystemMessagesForServer>, // High-priority system messages.
    client_receiver: Receiver<ClientMessagesForServer>, // Low-priority client requests.
}

impl Server {
    /// Creates a new Server instance.
    pub fn new(
        reservation_timeout: u32,
        database_helper: DatabaseHelper,
        coordinator: CoordinatorHelper,
    ) -> Server {
        let id = Uuid::new_v4(); // Generate a unique ID for the server.
        let tickets = database_helper.allocate(TICKET_BATCH_SIZE); // Allocate initial tickets.

        // Create channels for system and client messages.
        let (system_sender, system_receiver) = flume::unbounded();
        let (client_sender, client_receiver) = flume::unbounded();

        Self {
            id,
            available_tickets: Arc::new(Mutex::new(tickets)),
            reservations: Arc::new(Mutex::new(HashMap::new())),
            reservation_timeout,
            database_helper,
            coordinator,
            is_shutting_down: Arc::new(Mutex::new(false)), // Initialize to false
            fully_shut_down: Arc::new(Mutex::new(false)), // Initialize to false
            system_sender,
            client_sender,
            system_receiver,
            client_receiver,
        }
    }

    /// Starts the server in two separate threads for system and client messages.
    pub fn start(self) {
        // Clone Arc references for use inside the threads.
        let is_shutting_down = Arc::clone(&self.is_shutting_down);
        let fully_shut_down = Arc::clone(&self.fully_shut_down);

        let system_receiver = self.system_receiver.clone();
        let server_clone_for_system = self.clone();

        // Thread for system messages.
        thread::spawn(move || {
            loop {
                match system_receiver.recv() {
                    Ok(system_message) => {
                        server_clone_for_system.handle_system_message(system_message);
                    }
                    Err(_) => {
                        break;
                    }
                }

                // Check if the server is shutting down and can terminate.
                let can_shutdown = {
                    let flag = is_shutting_down.lock().unwrap();
                    let fully = fully_shut_down.lock().unwrap();
                    *flag && !*fully && server_clone_for_system.reservations.lock().unwrap().is_empty()
                };
                if can_shutdown {
                    // Deallocate any remaining tickets
                    let tickets: Vec<u32> = {
                        let mut available = server_clone_for_system.available_tickets.lock().unwrap();
                        let tickets = available.clone();
                        available.clear();
                        tickets
                    };
                    server_clone_for_system.database_helper.deallocate(&tickets);
                    server_clone_for_system.coordinator.remove_shutting_down_server(server_clone_for_system.id);

                    // Set the fully_shut_down flag to true to prevent further deallocation attempts.
                    {
                        let mut fully = fully_shut_down.lock().unwrap();
                        *fully = true;
                    }

                    // Properly drop the client_sender to close the channel.
                    drop(server_clone_for_system.client_sender.clone());
                }
            }
        });

        let client_receiver = self.client_receiver.clone();
        let server_clone_for_client = self.clone();

        // Thread for client messages.
        thread::spawn(move || {
            loop {
                // Periodically check for expired reservations and remove them.
                server_clone_for_client.clear_reservations();

                match client_receiver.recv_timeout(Duration::from_millis(100)) {
                    Ok(client_message) => {
                        server_clone_for_client.handle_client_message(client_message);
                    }
                    Err(flume::RecvTimeoutError::Timeout) => {
                        // Timeout occurred, continue to the next iteration to check reservations.
                    }
                    Err(flume::RecvTimeoutError::Disconnected) => {
                        break;
                    }
                }
            }
        });
    }

    /// Handles system messages.
    fn handle_system_message(&self, system_message: SystemMessagesForServer) {
        match system_message {
            SystemMessagesForServer::Id(sender) => {
                sender.send(self.id).unwrap();
            }
            SystemMessagesForServer::HandleEstimator(sender) => {
                let available_tickets = self.handle_estimator();
                sender.send(available_tickets).unwrap();
            }
            SystemMessagesForServer::Shutdown => {
                self.shutdown();
            }
            SystemMessagesForServer::CanShutdown(sender) => {
                let can_shutdown = self.can_shutdown();
                sender.send(can_shutdown).unwrap();
            }
            SystemMessagesForServer::ForceShutdown => {
                let mut flag = self.is_shutting_down.lock().unwrap();
                *flag = true;
            }
        }
    }

    /// Handles client messages.
    fn handle_client_message(&self, client_message: ClientMessagesForServer) {
        match client_message {
            ClientMessagesForServer::HandleRequest(rq) => {
                self.handle_request(rq);
            }
        }
    }

    /// Handles different types of client requests.
    fn handle_request(&self, mut rq: Request) {
        // Set the server_id to this server's ID by default
        rq.set_server_id(self.id);

        match rq.kind() {
            RequestKind::NumAvailableTickets => {
                let available = self.available_tickets.lock().unwrap().len() as u32;
                rq.respond_with_int(available);
            }
            RequestKind::ReserveTicket => {
                self.handle_reserve_ticket(rq);
            }
            RequestKind::BuyTicket => {
                self.handle_buy_ticket(rq);
            }
            RequestKind::AbortPurchase => {
                self.handle_abort_purchase(rq);
            }
            _ => {
                rq.respond_with_err("Invalid request kind!");
            }
        }
    }

    fn handle_reserve_ticket(&self, mut rq: Request) {
        // Check if shutting down
        let shutting_down = {
            let flag = self.is_shutting_down.lock().unwrap();
            *flag
        };
    
        if shutting_down {
            // Find another server
            let new_server_id = if let Some(other_server) = self.coordinator.pick_random_server() {
                other_server.id()
            } else {
                // No other servers available, set server_id to Uuid::nil()
                Uuid::nil()
            };
    
            // Set the server_id in the request (assuming it will be used in the response)
            rq.set_server_id(new_server_id);
    
            // Respond with the exact error message expected by the test
            rq.respond_with_err("This server is terminating");
            return;
        }

        // Existing logic for handling reservations
        let mut available = self.available_tickets.lock().unwrap();
        let mut reservations = self.reservations.lock().unwrap();
        let customer = rq.customer_id();

        match reservations.entry(customer) {
            Entry::Occupied(_) => {
                rq.respond_with_err("A ticket has already been reserved!");
            }
            Entry::Vacant(entry) => {
                if available.is_empty() {
                    let tickets = self.database_helper.allocate(TICKET_BATCH_SIZE);
                    if tickets.is_empty() {
                        rq.respond_with_sold_out();
                        return;
                    }
                    available.extend(tickets);
                }
                if let Some(ticket) = available.pop() {
                    entry.insert(Reservation::new(ticket));
                    rq.respond_with_int(ticket);
                }
            }
        }
    }

    /// Handles ticket purchase requests.
    fn handle_buy_ticket(&self, mut rq: Request) {
        let mut reservations = self.reservations.lock().unwrap();
        let cid = rq.customer_id();

        if let Some(ticket) = rq.read_u32() {
            if let Some(res) = reservations.get(&cid) {
                if ticket != res.ticket {
                    rq.respond_with_err("Invalid ticket ID provided!");
                } else {
                    reservations.remove(&cid);
                    rq.respond_with_int(ticket);
                }
            } else {
                rq.respond_with_err("No ticket has been reserved!");
            }
        } else {
            rq.respond_with_err("No ticket ID provided!");
        }
    }

    /// Handles ticket purchase abortion requests.
    fn handle_abort_purchase(&self, mut rq: Request) {
        let mut reservations = self.reservations.lock().unwrap();
        let cid = rq.customer_id();

        if let Some(ticket) = rq.read_u32() {
            if let Some(res) = reservations.get(&cid) {
                if ticket != res.ticket {
                    rq.respond_with_err("Invalid ticket ID provided!");
                } else {
                    reservations.remove(&cid);
                    self.available_tickets.lock().unwrap().push(ticket);
                    rq.respond_with_int(ticket);
                }
            } else {
                rq.respond_with_err("No ticket has been reserved!");
            }
        } else {
            rq.respond_with_err("No ticket ID provided!");
        }
    }

    /// Clears expired reservations.
    fn clear_reservations(&self) {
        let now = Instant::now();
        let mut reservations = self.reservations.lock().unwrap();

        let expired_reservations: Vec<(Uuid, u32)> = reservations
            .iter()
            .filter(|(_, res)| now.duration_since(res.reserved_at) >= Duration::from_secs(self.reservation_timeout as u64))
            .map(|(&cid, res)| (cid, res.ticket))
            .collect();

        if !expired_reservations.is_empty() {
            let is_shutting_down = {
                let flag = self.is_shutting_down.lock().unwrap();
                *flag
            };

            if is_shutting_down {
                // Deallocate tickets back to database
                let tickets_to_deallocate: Vec<u32> = expired_reservations.iter().map(|(_, ticket)| *ticket).collect();
                self.database_helper.deallocate(&tickets_to_deallocate);
            } else {
                let mut available = self.available_tickets.lock().unwrap();
                for (_, ticket) in &expired_reservations {
                    available.push(*ticket);
                }
            }

            for (cid, _) in &expired_reservations {
                reservations.remove(cid);
            }
        }
    }

    /// Initiates server shutdown.
    fn shutdown(&self) {
        // Set the shutdown flag first to prevent new reservations
        {
            let mut flag = self.is_shutting_down.lock().unwrap();
            if *flag {
                return;
            }
            *flag = true;
        }

        // Immediately deallocate all non-reserved tickets.
        {
            let mut available = self.available_tickets.lock().unwrap();
            if !available.is_empty() {
                let tickets: Vec<u32> = available.clone();
                available.clear();
                self.database_helper.deallocate(&tickets);
            }
        }
    }

    /// Checks if the server can shut down.
    fn can_shutdown(&self) -> bool {
        let flag = self.is_shutting_down.lock().unwrap();
        let fully = self.fully_shut_down.lock().unwrap();
        *flag && !*fully && self.reservations.lock().unwrap().is_empty()
    }

    /// Handles estimation requests by returning the number of available tickets.
    fn handle_estimator(&self) -> u32 {
        self.clear_reservations(); // Clear expired reservations.
        let available = self.available_tickets.lock().unwrap().len() as u32;
        available
    }

    /// Retrieves a ServerHelper for interacting with this server.
    pub fn get_server_helper(&self) -> ServerHelper {
        ServerHelper {
            system_sender: self.system_sender.clone(), // Clone the system_sender.
            client_sender: self.client_sender.clone(), // Clone the client_sender.
        }
    }
}

/// Helper struct to interact with the server from other threads.
#[derive(Clone)]
pub struct ServerHelper {
    system_sender: Sender<SystemMessagesForServer>, // Channel for system messages.
    client_sender: Sender<ClientMessagesForServer>, // Channel for client requests.
}

impl ServerHelper {
    /// Get the server's ID by sending a message and receiving a response.
    pub fn id(&self) -> Uuid {
        let (sender, receiver) = flume::unbounded();
        self.system_sender
            .send(SystemMessagesForServer::Id(sender))
            .unwrap();
        let id = receiver.recv().unwrap();
        id
    }

    /// Handle a client request by forwarding it to the server.
    pub fn handle_request(&self, rq: Request) {
        self.client_sender
            .send(ClientMessagesForServer::HandleRequest(rq))
            .unwrap();
    }

    /// Handle a request from the estimator by forwarding it to the server.
    pub fn handle_estimator(&self) -> u32 {
        let (sender, receiver) = flume::unbounded();
        self.system_sender
            .send(SystemMessagesForServer::HandleEstimator(sender))
            .unwrap();
        receiver.recv().unwrap()
    }

    /// Trigger the server to begin shutdown.
    pub fn shutdown(&self) {
        self.system_sender
            .send(SystemMessagesForServer::Shutdown)
            .unwrap();
    }

    /// Check if the server can shut down.
    pub fn can_shutdown(&self) -> bool {
        let (sender, receiver) = flume::unbounded();
        self.system_sender
            .send(SystemMessagesForServer::CanShutdown(sender))
            .unwrap();
        receiver.recv().unwrap()
    }

    /// Force the server to shut down immediately.
    pub fn force_shutdown(&self) {
        self.system_sender
            .send(SystemMessagesForServer::ForceShutdown)
            .unwrap();
    }
}

/// Struct representing a customer reservation.
#[derive(Debug, Clone)]
struct Reservation {
    ticket: u32,
    reserved_at: Instant,
}

impl Reservation {
    /// Creates a new Reservation with the specified ticket.
    fn new(ticket: u32) -> Self {
        Self {
            ticket,
            reserved_at: Instant::now(),
        }
    }
}