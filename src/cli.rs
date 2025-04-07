use clap::Parser;
use shellexpand;

/// Returns the default path for nodes, expanding the tilde.
fn default_node_path() -> String {
    shellexpand::tilde("~/.local/share/autonomi/node/*").into_owned()
}

/// Returns the default glob pattern for log files, expanding the tilde.
fn default_log_path() -> String {
    shellexpand::tilde("~/.local/share/autonomi/node/*/logs/antnode.log").into_owned()
}

/// Command-line arguments
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// Glob pattern for the root directories of the running nodes (e.g., "/path/to/nodes/node-*")
    #[arg(long, default_value_t = default_node_path())]
    pub path: String,

    /// Glob pattern for the log files to scan for metrics server addresses (e.g., "/path/to/nodes/node-*/logs/antnode.log")
    /// If not specified, it defaults to the node path appended with "/logs/antnode.log".
    #[arg(long)]
    pub log_path: Option<String>,
}
