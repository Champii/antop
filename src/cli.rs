use clap::Parser;

/// Command-line arguments
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// Glob pattern for the root directories of the running nodes (e.g., "/path/to/nodes/node-*")
    #[arg(long, default_value = "~/.local/share/autonomi/node/*")]
    pub path: String,
}
