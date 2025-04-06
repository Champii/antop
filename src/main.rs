mod app;
mod cli;
mod discovery;
mod fetch;
mod metrics;
mod ui;

use anyhow::Result;
use clap::Parser;

use crate::{
    app::App,
    cli::Cli,
    discovery::find_metrics_servers,
    ui::{restore_terminal, run_app, setup_terminal},
};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let initial_servers = match find_metrics_servers(&cli.path) {
        Ok(servers) => {
            if servers.is_empty() {
                // Log to stderr if no servers found initially
                eprintln!(
                    "Warning: No metrics servers found in logs. Waiting for discovery or manual config."
                );
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
            return Err(e);
        }
    };

    let mut terminal = setup_terminal()?;

    let app = App::new(initial_servers, cli.path.clone());

    let run_result = run_app(&mut terminal, app, &cli).await;

    // Restore the terminal state using the ui module, regardless of run_result
    restore_terminal(&mut terminal)?;

    if let Err(err) = run_result {
        eprintln!("Application error: {:?}", err);
        return Err(err);
    }

    Ok(())
}
