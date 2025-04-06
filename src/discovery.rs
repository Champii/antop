use anyhow::{Context, Result};
use glob::glob;
use regex::Regex;
use std::{fs, path::PathBuf};

/// Finds antnode log files and extracts the last reported metrics server address from each.
/// Uses the provided `log_pattern_override` if Some, otherwise defaults to ~/.local/share/autonomi/node/*/logs/antnode.log
pub fn find_metrics_servers(log_pattern_override: Option<&str>) -> Result<Vec<(String, String)>> {
    let pattern_str = match log_pattern_override {
        Some(pattern) => pattern.to_string(),
        None => {
            let home_dir = dirs::home_dir().context("Failed to get home directory")?;
            home_dir
                .join(".local/share/autonomi/node/*/logs/antnode.log")
                .to_string_lossy()
                .to_string()
        }
    };

    let re = Regex::new(r"Metrics server on (\S+)")?;
    let mut servers: Vec<(String, String)> = Vec::new();

    for entry in glob(&pattern_str).context("Failed to read glob pattern")? {
        match entry {
            Ok(path) => {
                if path.is_file() {
                    // Extract server name from path: .../node/<server_name>/logs/antnode.log
                    let server_name = path
                        .parent() // .../node/<server_name>/logs
                        .and_then(|p| p.parent()) // .../node/<server_name>
                        .and_then(|p| p.file_name()) // <server_name>
                        .and_then(|os_str| os_str.to_str())
                        .map(|s| s.to_string());

                    if let Some(name) = server_name {
                        match process_log_file(&path, &re) {
                            Ok(Some(address)) => {
                                servers.push((name, address));
                            }
                            Ok(None) => {
                                // Log file found, but no metrics address inside
                                // eprintln!("Warning: No metrics address found in log file: {:?}", path);
                            }
                            Err(e) => {
                                // Error reading or processing the log file content
                                eprintln!("Error processing log file {:?}: {}", path, e);
                            }
                        }
                    } else {
                        // Path structure didn't allow extracting a name
                        eprintln!(
                            "Warning: Could not extract server name from path: {:?}",
                            path
                        );
                    }
                }
            }
            Err(e) => eprintln!("Error processing glob entry: {}", e),
        }
    }

    servers.sort_by(|a, b| a.0.cmp(&b.0));
    servers.dedup_by(|a, b| a.1 == b.1);
    Ok(servers)
}

/// Reads a single log file and extracts the last metrics server address.
fn process_log_file(path: &PathBuf, re: &Regex) -> Result<Option<String>> {
    let content =
        fs::read_to_string(path).with_context(|| format!("Failed to read log file: {:?}", path))?;
    let mut last_match: Option<String> = None;
    for line in content.lines() {
        if let Some(caps) = re.captures(line) {
            if let Some(address) = caps.get(1) {
                last_match = Some(address.as_str().to_string());
            }
        }
    }
    Ok(last_match)
}
