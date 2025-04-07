mod app;
mod cli;
mod discovery;
mod fetch;
mod metrics;
mod ui;

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

use crate::{
    app::App,
    cli::Cli,
    discovery::find_metrics_servers,
    ui::{restore_terminal, run_app, setup_terminal},
};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Determine the log path: use provided or derive from node path
    let effective_log_path = cli.log_path.clone().unwrap_or_else(|| {
        // Derive from cli.path
        let mut path_buf = PathBuf::from(&cli.path);
        if path_buf.file_name().map_or(false, |name| name == "*") {
            path_buf.pop(); // Remove the "*"
            path_buf.push("*"); // Re-add it explicitly before appending further
        }
        path_buf.push("logs");
        path_buf.push("antnode.log");
        path_buf.to_string_lossy().into_owned()
    });

    let initial_servers = match find_metrics_servers(&effective_log_path) {
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

    let run_result = run_app(&mut terminal, app, &cli, &effective_log_path).await;

    // Restore the terminal state using the ui module, regardless of run_result
    restore_terminal(&mut terminal)?;

    if let Err(err) = run_result {
        // Keep this final error print as it happens after UI closes
        eprintln!("Application error: {:?}", err);
        return Err(err);
    }

    Ok(())
}
