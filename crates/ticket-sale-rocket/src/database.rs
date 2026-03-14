use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use flume::{Receiver, Sender};
use uuid::Uuid;

/// Enum representing the different types of messages the Database can handle.
#[derive(Debug)] // Adding Debug to easily log message types.
pub enum MessagesForDatabase {
    Allocate(u32, Sender<Vec<u32>>), // Allocate a specified number of tickets.
    GetNumAvailable(Sender<u32>),    // Get the number of available tickets.
    Deallocate(Vec<u32>),            // Deallocate a set of tickets.
}

/// The central database that holds and allocates tickets.d
#[derive(Clone)]
pub struct Database {
    /// List of available tickets that have not yet been allocated by any server.
    unallocated: Arc<Mutex<Vec<u32>>>,

    receiver: Receiver<MessagesForDatabase>, /* A flume receiver to handle incoming
                                              * database-related messages. */
    sender: Sender<MessagesForDatabase>, // A flume sender for sending messages to the database.
}

impl Database {
    /// Creates a new Database instance with the specified number of tickets.
    pub fn new(num_tickets: u32) -> Self {
        let unallocated: Vec<u32> = (0..num_tickets).collect();
        let (sender, receiver) = flume::unbounded();

        Self {
            unallocated: Arc::new(Mutex::new(unallocated)),
            sender,
            receiver,
        }
    }

    /// Starts the database in a separate thread to process incoming messages.
    pub fn start(self) {
        let unallocated = Arc::clone(&self.unallocated);
        let receiver = self.receiver.clone();

        thread::spawn(move || {
            loop {
                match receiver.recv() {
                    Ok(message) => {
                        match message {
                            MessagesForDatabase::Allocate(num_tickets, sender) => {
                                let allocated = Self::allocate(&unallocated, num_tickets);
                                if let Err(_) = sender.send(allocated) {}
                            }
                            MessagesForDatabase::GetNumAvailable(sender) => {
                                let num_available = Self::get_num_available(&unallocated);
                                if let Err(_) = sender.send(num_available) {}
                            }
                            MessagesForDatabase::Deallocate(tickets) => {
                                Self::deallocate(&unallocated, &tickets);
                            }
                        }
                    }
                    Err(_) => {
                        break;
                    }
                }
            }
        });
    }

    /// Returns a DatabaseHelper to interact with this Database.
    pub fn get_database_helper(&self) -> DatabaseHelper {
        DatabaseHelper {
            sender: self.sender.clone(),
        }
    }

    /// Gets the number of available tickets.
    fn get_num_available(unallocated: &Arc<Mutex<Vec<u32>>>) -> u32 {
        let unallocated_lock = unallocated.lock().unwrap();
        unallocated_lock.len() as u32
    }

    /// Allocates the specified number of tickets.
    fn allocate(unallocated: &Arc<Mutex<Vec<u32>>>, num_tickets: u32) -> Vec<u32> {
        let mut unallocated_lock = unallocated.lock().unwrap();
        let available = unallocated_lock.len() as u32;
        let allocate_count = num_tickets.min(available);

        if allocate_count == 0 {
            return Vec::new();
        }

        let allocate_count_usize = allocate_count as usize;
        let split = unallocated_lock.len() - allocate_count_usize;
        let allocated = unallocated_lock.split_off(split);
        allocated
    }

    /// Deallocates the specified tickets, returning them to the pool of available
    /// tickets.
    fn deallocate(unallocated: &Arc<Mutex<Vec<u32>>>, tickets: &[u32]) {
        let mut unallocated_lock = unallocated.lock().unwrap();
        unallocated_lock.extend_from_slice(tickets);
    }
}

/// Helper struct to interact with the Database using messages.
#[derive(Clone)]
pub struct DatabaseHelper {
    sender: Sender<MessagesForDatabase>, // The sender for sending messages to the database.
}

impl DatabaseHelper {
    /// Creates a new DatabaseHelper.
    pub fn new(sender: Sender<MessagesForDatabase>) -> Self {
        Self { sender }
    }

    /// Gets the number of available tickets.
    pub fn get_num_available(&self) -> u32 {
        let (sender, receiver) = flume::bounded(1);
        if let Err(_) = self
            .sender
            .send(MessagesForDatabase::GetNumAvailable(sender))
        {
            return 0;
        }

        match receiver.recv() {
            Ok(num) => num,
            Err(_) => 0,
        }
    }

    /// Allocates the specified number of tickets.
    pub fn allocate(&self, num_tickets: u32) -> Vec<u32> {
        let (sender, receiver) = flume::bounded(1);
        if let Err(_) = self
            .sender
            .send(MessagesForDatabase::Allocate(num_tickets, sender))
        {
            return Vec::new();
        }

        match receiver.recv() {
            Ok(tickets) => tickets,
            Err(_) => Vec::new(),
        }
    }

    /// Deallocates the specified tickets, returning them to the pool of available
    /// tickets.
    pub fn deallocate(&self, tickets: &[u32]) {
        let tickets_vec = tickets.to_vec();
        let _ = self
            .sender
            .send(MessagesForDatabase::Deallocate(tickets_vec));
    }
}
