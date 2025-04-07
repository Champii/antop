mod app;
mod cli;
mod discovery;
mod fetch;
mod metrics;
mod ui;

use anyhow::{Context, Result};
use clap::Parser;
use std::path::PathBuf;

use crate::{
    app::App,
    cli::Cli,
    discovery::{find_metrics_nodes, find_node_directories},
    ui::{restore_terminal, run_app, setup_terminal},
};

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let cli = Cli::parse();

    // Expand the tilde in the path provided by the user
    let expanded_path_glob = shellexpand::tilde(&cli.path).into_owned();

    // --- New: Find all node directories first ---
    let discovered_node_dirs = find_node_directories(&expanded_path_glob)
        .context("Failed to find node directories based on the provided path pattern")?;

    if discovered_node_dirs.is_empty() {
        eprintln!(
            "Warning: No node directories found matching the pattern: {}. Ensure the path is correct and nodes exist.",
            expanded_path_glob
        );
        // Decide if we should exit or continue with an empty list
        // For now, let's continue, the app might handle an empty list gracefully.
    }
    // --- End New ---

    // Determine the log path: use provided or derive from node path
    let effective_log_path = match cli.log_path.as_ref() {
        Some(log_path) => shellexpand::tilde(log_path).into_owned(), // Expand tilde if provided
        None => {
            // Derive log path based on the *original* potentially wildcarded path pattern
            let mut path_buf = PathBuf::from(&expanded_path_glob); // Use original glob pattern
            if path_buf.file_name().is_some_and(|name| name == "*") {
                // If the pattern ends with '*', assume it means node-*/logs/antnode.log
                path_buf.pop(); // Remove '*'
                path_buf.push("*"); // Add it back (or ensure it's there)
            } else {
                // If it's a specific directory, append /logs/antnode.log
                // This derivation logic might need refinement depending on expected usage
                // For now, let's keep the wildcard assumption as it was implicitly there before
                if !expanded_path_glob.ends_with('*') {
                    // Add wildcard if not present, mimicking the old behavior's likely intent
                    // This part is tricky; ideally, log path derivation is explicit or simpler.
                    // Let's stick to the original logic's implicit behavior for now.
                    path_buf.push("*");
                }
            }
            path_buf.push("logs");
            path_buf.push("antnode.log");
            path_buf.to_string_lossy().into_owned()
        }
    };

    // Convert the effective_log_path String to PathBuf
    let log_path_buf = PathBuf::from(effective_log_path.clone());

    // Find initial metrics URLs
    let initial_node_urls = match find_metrics_nodes(log_path_buf).await {
        Ok(nodes) => {
            if nodes.is_empty() && !discovered_node_dirs.is_empty() {
                // Only warn if we found directories but no metrics URLs
                eprintln!(
                    "Warning: Found {} node directories but no metrics servers via logs: {}",
                    discovered_node_dirs.len(),
                    effective_log_path
                );
                eprintln!("Nodes will be shown as stopped/pending until metrics are found.");
            }
            nodes // This is Vec<(root_path, url)>
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

    // Create the App state
    // Pass the discovered directories *and* the initial URLs
    let app = App::new(
        discovered_node_dirs,
        initial_node_urls,
        expanded_path_glob.clone(),
    );

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
