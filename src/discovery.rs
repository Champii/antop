use anyhow::{Context, Result};
use glob::glob;
use regex::Regex;
use std::{fs, path::PathBuf};

/// Finds node directories matching the glob pattern, then looks for log files within them
/// to extract the last reported metrics server address from each.
pub fn find_metrics_servers(node_path_glob: &str) -> Result<Vec<(String, String)>> {
    let re = Regex::new(r"Metrics server on (\S+)")?;
    let mut servers: Vec<(String, String)> = Vec::new();

    for entry in glob(node_path_glob).context("Failed to read node path glob pattern")? {
        match entry {
            Ok(node_dir) => {
                if node_dir.is_dir() {
                    let log_file_path = node_dir.join("logs").join("antnode.log");

                    // Extract server name from the node directory path
                    let server_name = node_dir
                        .file_name()
                        .and_then(|os_str| os_str.to_str())
                        .map(|s| s.to_string());

                    if let Some(name) = server_name {
                        if log_file_path.is_file() {
                            match process_log_file(&log_file_path, &re) {
                                Ok(Some(address)) => {
                                    servers.push((name.clone(), address));
                                }
                                Ok(None) => {
                                    // Log file exists, but no metrics address inside
                                }
                                Err(_err) => {
                                    // Error reading or processing the log file content
                                }
                            }
                        } else {
                            // Log file doesn't exist for this node directory
                        }
                    } else {
                        // Path structure didn't allow extracting a name
                    }
                }
            }
            Err(_e) => { /* Optionally log this error elsewhere, e.g., to a file */ }
        }
    }

    servers.sort_by(|a, b| a.0.cmp(&b.0));
    // Note: Deduping by address might hide multiple nodes reporting the same address.
    // Consider if this is the desired behavior.
    servers.dedup_by(|a, b| a.1 == b.1);
    Ok(servers)
}

/// Reads a single log file and extracts the last metrics server address.
fn process_log_file(path: &PathBuf, re: &Regex) -> Result<Option<String>> {
    let content =
        fs::read_to_string(path).with_context(|| format!("Failed to read log file: {:?}", path))?;
    let mut last_match: Option<String> = None;
    for line in content.lines().take(40) {
        if let Some(caps) = re.captures(line) {
            if let Some(address) = caps.get(1) {
                last_match = Some(address.as_str().to_string());
            }
        }
    }
    Ok(last_match)
}
