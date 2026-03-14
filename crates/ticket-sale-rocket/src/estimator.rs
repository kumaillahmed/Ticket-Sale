use std::{thread, time::Duration};
use uuid::Uuid;
use crate::{coordinator::CoordinatorHelper, database::DatabaseHelper};
use flume::{Receiver, Sender};
use std::collections::HashMap;

/// Estimator that estimates the number of tickets available overall.
pub struct Estimator {
    database: DatabaseHelper, // A helper to interact with the database.
    coordinator: CoordinatorHelper, // A helper to interact with the coordinator.
    roundtrip_secs: u32, // The time (in seconds) the estimator needs to complete a round.
    shutdown_receiver: Receiver<()>, // A flume receiver to listen for shutdown signals.
    shutdown_sender: Sender<()>, // A flume sender to send a shutdown signal.
}

impl Estimator {
    /// Creates a new Estimator instance.
    pub fn new(
        database: DatabaseHelper,
        coordinator: CoordinatorHelper,
        roundtrip_secs: u32,
    ) -> Self {
        let (shutdown_sender, shutdown_receiver) = flume::bounded(1);
        Self {
            database,
            coordinator,
            roundtrip_secs,
            shutdown_receiver,
            shutdown_sender,
        }
    }

    /// Starts the estimator in a new thread.
    pub fn start(&self) {
        let shutdown_receiver = self.shutdown_receiver.clone();
        let database = self.database.clone();
        let coordinator = self.coordinator.clone();
        let roundtrip_secs = self.roundtrip_secs;

        thread::spawn(move || {
            loop {
                // Check for shutdown signal
                match shutdown_receiver.recv_timeout(Duration::from_millis(100)) {
                    Ok(_) | Err(flume::RecvTimeoutError::Disconnected) => {
                        break;
                    }
                    Err(flume::RecvTimeoutError::Timeout) => {}
                }

                // Fetch all server IDs
                let servers = coordinator.get_all_server_ids();
                let num_of_servers = servers.len();

                if num_of_servers == 0 {
                    thread::sleep(Duration::from_secs(roundtrip_secs as u64));
                    continue;
                }

                // Calculate sleep duration per server to distribute roundtrip time
                let sleep_duration_secs = roundtrip_secs as f64 / num_of_servers as f64;
                let sleep_duration = Duration::from_secs_f64(sleep_duration_secs);

                let mut total_available = 0;

                for server_id in &servers {
                    // Check for shutdown signal during iteration
                    match shutdown_receiver.recv_timeout(Duration::from_millis(1)) {
                        Ok(_) | Err(flume::RecvTimeoutError::Disconnected) => {
                            break;
                        }
                        Err(flume::RecvTimeoutError::Timeout) => {}
                    }

                    // Sleep to simulate roundtrip time per server
                    thread::sleep(sleep_duration);

                    // Query server for its available tickets
                    if let Some(server_helper) = coordinator.get_server(*server_id) {
                        total_available += server_helper.handle_estimator();
                    }
                }

                // Fetch available tickets from the Database
                let database_available = database.get_num_available();

                let total_tickets_available = total_available + database_available;

                // Optionally, store or act upon the total_tickets_available here
            }
        });
    }

    /// Provides a Sender to signal shutdown.
    pub fn get_shutdown_sender(&self) -> Sender<()> {
        self.shutdown_sender.clone()
    }
}