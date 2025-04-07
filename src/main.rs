mod app;
mod cli;
mod discovery;
mod fetch;
mod metrics;
mod ui;

use anyhow::Result;
use clap::Parser;
use shellexpand;
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

    // Expand the tilde in the path provided by the user
    let expanded_path = shellexpand::tilde(&cli.path).into_owned();

    // Determine the log path: use provided or derive from node path
    let effective_log_path = match cli.log_path.as_ref() {
        Some(log_path) => shellexpand::tilde(log_path).into_owned(), // Expand tilde if provided
        None => {
            // Derive from the potentially expanded node path
            let mut path_buf = PathBuf::from(&expanded_path);
            let mut re_add_wildcard = false;

            // Check if the last component is a wildcard
            if path_buf.file_name().map_or(false, |name| name == "*") {
                path_buf.pop(); // Remove the "*"
                re_add_wildcard = true;
            }

            // Re-add wildcard if it was present, ensuring the glob structure
            if re_add_wildcard {
                path_buf.push("*");
            }

            path_buf.push("logs");
            path_buf.push("antnode.log");
            path_buf.to_string_lossy().into_owned()
        }
    };

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

    let app = App::new(initial_servers, expanded_path.clone()); // Use expanded_path

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
