use ticket_sale_core::Config;
use crate::balancer::MessagesForBalancer;
use crate::coordinator::Coordinator;
use crate::database::Database;
use crate::estimator::Estimator;
use flume::unbounded;

mod balancer;
mod constants;
mod coordinator;
mod database;
mod estimator;
mod server;

pub use balancer::Balancer;

/// The launch function initializes and starts all the components of the system.
pub fn launch(config: &Config) -> Balancer {
    if config.bonus {
        todo!("Bonus not implemented!")
        // Not implementing it.
    }

    // Create a new Database instance with the specified number of tickets from the configuration.
    let database = Database::new(config.tickets);

    // Get a DatabaseHelper, which provides a thread-safe way to interact with the database.
    let database_helper = database.get_database_helper();
    database.start();

    // Create channels for Coordinator to Balancer communication.
    let (balancer_sender, balancer_receiver) = unbounded::<MessagesForBalancer>();

    // Initialize the Coordinator with the provided reservation timeout, database helper, and number of initial servers.
    let coordinator = Coordinator::new(
        config.timeout,
        database_helper.clone(),
        config.initial_servers,
        balancer_sender.clone(), // Pass the Balancer sender.
    );

    // Create a new Estimator that will monitor the overall ticket distribution across servers.
    let estimator = Estimator::new(
        database_helper.clone(),
        coordinator.get_coordinator_helper(),
        config.timeout,
    );

    // Create a new Balancer, which will handle incoming requests and distribute them to the appropriate servers.
    let balancer = Balancer::new(
        coordinator.get_coordinator_helper(),
        estimator.get_shutdown_sender(),
        balancer_receiver, // Pass the Balancer receiver.
    );

    coordinator.start();
    estimator.start();

    // Return the Balancer instance, which will handle incoming requests and distribute them to servers.
    balancer
}