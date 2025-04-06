use anyhow::{Context, Result};
use glob::glob;
use regex::Regex;
use std::{
    fs,
    path::{Path, PathBuf},
};

/// Finds metrics server addresses by scanning log files specified by the glob pattern.
/// Extracts server name from the parent directory of the log file.
pub fn find_metrics_servers(log_path_glob: &str) -> Result<Vec<(String, String)>> {
    let re = Regex::new(r"Metrics server on (\S+)")?;
    let mut servers: Vec<(String, String)> = Vec::new();

    for entry in glob(log_path_glob).context("Failed to read log path glob pattern")? {
        match entry {
            Ok(log_file_path) => {
                if log_file_path.is_file() {
                    // Try to get the parent directory of the log file
                    if let Some(parent_dir) = log_file_path.parent() {
                        // Extract server name from the parent directory's name
                        let server_name = parent_dir
                            .file_name()
                            .and_then(|os_str| os_str.to_str())
                            .map(|s| s.to_string());

                        if let Some(name) = server_name {
                            match process_log_file(&log_file_path, &re) {
                                Ok(Some(address)) => {
                                    servers.push((name.clone(), address));
                                }
                                Ok(None) => {
                                    // Log file processed, but no metrics address found
                                }
                                Err(_err) => {
                                    // Error reading or processing this specific log file
                                    // Optionally log this error
                                }
                            }
                        } else {
                            // Could not determine server name from parent directory path
                            // Optionally log this
                        }
                    } else {
                        // Could not get parent directory for the log file
                        // Optionally log this
                    }
                }
            }
            Err(_e) => { /* Optionally log glob pattern error */ }
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
    // Limit lines read for performance, especially on large logs.
    // Increased slightly from 40, just in case.
    for line in content.lines().take(50) {
        if let Some(caps) = re.captures(line) {
            if let Some(address) = caps.get(1) {
                last_match = Some(address.as_str().to_string());
            }
        }
    }
    Ok(last_match)
}
