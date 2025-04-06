use clap::Parser;
use shellexpand;

/// Returns the default path for nodes, expanding the tilde.
fn default_path() -> String {
    shellexpand::tilde("~/.local/share/autonomi/node/*").into_owned()
}

/// Command-line arguments
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// Glob pattern for the root directories of the running nodes (e.g., "/path/to/nodes/node-*")
    #[arg(long, default_value_t = default_path())]
    pub path: String,
}
