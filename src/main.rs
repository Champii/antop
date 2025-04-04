// Declare the modules
mod app;
mod cli;
mod discovery;
mod fetch;
mod metrics;
mod ui;

use anyhow::Result;
use clap::Parser;

// Import necessary items from the modules
use crate::{
    app::App,
    cli::Cli,
    discovery::find_metrics_servers,
    ui::{restore_terminal, run_app, setup_terminal},
};

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command-line arguments using the cli module
    let cli = Cli::parse();

    // Initial server discovery using the discovery module
    let initial_servers = match find_metrics_servers(cli.logs.as_deref()) {
        Ok(servers) => {
            if servers.is_empty() {
                // Log to stderr if no servers found initially
                eprintln!("Warning: No metrics servers found in logs. Waiting for discovery or manual config.");
                // Proceed with an empty list; the app loop will handle discovery later
                Vec::new()
            } else {
                // Log to stdout before TUI starts
                println!("Found {} initial metrics servers.", servers.len());
                servers
            }
        }
        Err(e) => {
            // Log critical error and exit if initial discovery fails
            eprintln!("Error finding initial metrics servers: {}. Exiting.", e);
            return Err(e); // Propagate the error
        }
    };

    // Setup the terminal using the ui module
    let mut terminal = setup_terminal()?;

    // Create the application state using the app module
    let app_instance = App::new(initial_servers);

    // Run the main application loop using the ui module
    // Pass the terminal, app state, and cli args
    let run_result = run_app(&mut terminal, app_instance, &cli).await;

    // Restore the terminal state using the ui module, regardless of run_result
    restore_terminal(&mut terminal)?;

    // Handle potential errors from the application loop
    if let Err(err) = run_result {
        eprintln!("Application error: {:?}", err); // Log error to stderr
        return Err(err); // Propagate the error
    }

    // Exit successfully
    Ok(())
}
