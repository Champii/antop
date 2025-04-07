use anyhow::{Context, Result};
use glob::glob;
use regex::Regex;
use std::{fs, path::PathBuf};

/// Finds node root directories matching the provided glob pattern.
pub fn find_node_directories(path_glob: &str) -> Result<Vec<String>> {
    let mut directories = Vec::new();
    for entry in glob(path_glob).context("Failed to read node path glob pattern")? {
        match entry {
            Ok(path) => {
                // Ensure it's a directory before adding
                if path.is_dir() {
                    // Store the full path as the identifier
                    directories.push(path.to_string_lossy().to_string());
                }
            }
            Err(e) => {
                // Log or handle individual glob errors if necessary
                eprintln!("Warning: Error processing path entry: {}", e);
            }
        }
    }
    directories.sort(); // Keep the list sorted for consistency
    Ok(directories)
}

/// Finds metrics node addresses by scanning log files specified by the glob pattern.
/// Extracts node name from the parent directory of the log file.
pub async fn find_metrics_nodes(log_path_glob: PathBuf) -> Result<Vec<(String, String)>> {
    let re = Regex::new(r"Metrics server on (\S+)")?;
    let mut nodes: Vec<(String, String)> = Vec::new();

    // Convert PathBuf to string for glob, handle potential errors
    let glob_str = log_path_glob
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Log path is not valid UTF-8"))?;

    for entry in glob(glob_str).context("Failed to read log path glob pattern")? {
        match entry {
            Ok(log_file_path) => {
                if log_file_path.is_file() {
                    // Try to get the parent directory of the log file
                    if let Some(log_parent_dir) = log_file_path.parent() {
                        // Now, get the parent of the log's parent directory (the node's root)
                        if let Some(node_root_dir) = log_parent_dir.parent() {
                            // Use the full path of the node's root directory as the identifier
                            let root_path = node_root_dir.to_string_lossy().to_string();

                            match process_log_file(&log_file_path, &re) {
                                Ok(Some(address)) => {
                                    // Push the root_path and address
                                    nodes.push((root_path, address));
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
                            // Could not get parent of parent (e.g., log file is not in a 'logs' subdir?)
                            // Optionally log this, or perhaps fallback to log_parent_dir?
                            // For now, just skip if we can't get the node root dir this way.
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

    nodes.sort_by(|a, b| a.0.cmp(&b.0));
    // Note: Deduping by address might hide multiple nodes reporting the same address.
    // Consider if this is the desired behavior.
    nodes.dedup_by(|a, b| a.1 == b.1);
    Ok(nodes)
}

/// Reads a single log file and extracts the last metrics node address.
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
