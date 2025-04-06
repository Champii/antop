use clap::Parser;

/// Command-line arguments
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// Optional glob pattern for finding antnode log files (e.g., "/path/to/logs/**/*.log")
    #[arg(long)]
    pub logs: Option<String>,

    /// Base path for node storage directories relative to home (e.g., ".local/share/autonomi/node")
    #[arg(long, default_value = ".local/share/autonomi/node")]
    pub storage_base_path: String,
}
