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
use tokio;

use crate::{
    app::App,
    cli::Cli,
    discovery::find_metrics_nodes,
    ui::{restore_terminal, run_app, setup_terminal},
};

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
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

    // Convert the effective_log_path String to PathBuf, CLONE it here
    let log_path_buf = PathBuf::from(effective_log_path.clone());

    // Use .await for the async discovery function
    let initial_nodes = match find_metrics_nodes(log_path_buf).await {
        Ok(nodes) => {
            if nodes.is_empty() {
                eprintln!(
                    "Warning: No metrics servers found initially based on the log path pattern: {}",
                    effective_log_path
                );
                eprintln!("The application will continue and attempt discovery periodically.");
            }
            nodes
        }
        Err(e) => {
            eprintln!(
                "Error during initial metrics server discovery using pattern '{}': {}",
                effective_log_path, e
            );
            eprintln!(
                "Proceeding without initial servers. Discovery will be attempted periodically."
            );
            Vec::new() // Return an empty vector on error
        }
    };

    // Create the App state - remove mut
    let app = App::new(initial_nodes, expanded_path.clone()); // Pass the node path glob

    // Setup terminal
    let mut terminal = setup_terminal()?;

    // Run the main application loop using .await
    let app_result = run_app(&mut terminal, app, &cli, &effective_log_path).await;

    // Restore terminal state
    restore_terminal(&mut terminal)?;

    // Print any errors that occurred during the app run
    if let Err(err) = app_result {
        eprintln!("Error running application: {}", err);
        // Optionally, return a non-zero exit code here if desired
        // std::process::exit(1);
    }

    Ok(())
}
