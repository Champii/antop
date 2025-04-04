use anyhow::{Context, Result};
use glob::glob;
use regex::Regex;
use std::{fs, path::PathBuf};

/// Finds antnode log files and extracts the last reported metrics server address from each.
/// Uses the provided `log_pattern_override` if Some, otherwise defaults to ~/.local/share/autonomi/node/*/logs/antnode.log
pub fn find_metrics_servers(log_pattern_override: Option<&str>) -> Result<Vec<String>> {
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

    // This regex needs to be created here or passed in. Creating it here for simplicity.
    let re = Regex::new(r"Metrics server on (\S+)")?;
    let mut addresses = Vec::new();

    // Use glob to find matching log files
    for entry in glob(&pattern_str).context("Failed to read glob pattern")? {
        match entry {
            Ok(path) => {
                if path.is_file() {
                    match process_log_file(&path, &re) {
                        Ok(Some(address)) => {
                            addresses.push(address);
                        }
                        Ok(None) => {} // Ignore logs without the address
                        Err(e) => {
                            // Consider logging this error instead of printing directly
                            eprintln!("Error processing log file {:?}: {}", path, e);
                        }
                    }
                }
            }
            Err(e) => eprintln!("Error processing glob entry: {}", e), // Consider logging
        }
    }

    addresses.sort();
    addresses.dedup();
    Ok(addresses)
}

/// Reads a single log file and extracts the last metrics server address.
fn process_log_file(path: &PathBuf, re: &Regex) -> Result<Option<String>> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read log file: {:?}", path))?;
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