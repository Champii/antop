# Antop

<!-- Badges placeholder -->
[![Build Status](https://img.shields.io/github/actions/workflow/status/champii/antop/rust.yml?branch=master)](https://github.com/champii/antop/actions)
[![Crates.io](https://img.shields.io/crates/v/antop.svg)](https://crates.io/crates/antop)
[![License](https://img.shields.io/crates/l/antop.svg)](https://opensource.org/licenses/MIT) <!-- Or Apache-2.0 -->

A terminal dashboard for monitoring Autonomi node instances.

<!-- i want a img here -->
![Screenshot](./media/screenshot.png)


## Features

*   **Automatic Discovery:** Finds running `antnode` instances by scanning log files (configurable path).
*   **Real-time Metrics:** Fetches and displays key metrics from discovered nodes.
*   **TUI Dashboard:** Presents information clearly in your terminal using `ratatui`.

## Installation

### Prerequisites

*   [Rust](https://www.rust-lang.org/tools/install) (stable toolchain)

### From Source

1.  Clone the repository:
    ```bash
    git clone https://github.com/champii/antop.git # Replace with actual URL later
    cd antop
    ```
2.  Build the release binary:
    ```bash
    cargo build --release
    ```
3.  The executable will be located at `target/release/antop`.

### From Crates.io 

```bash
cargo install antop
```

## Usage

1.  Run the application:
    ```bash
    ./target/release/antop
    ```
    Or, if installed via `cargo install`:
    ```bash
    antop
    ```

2.  **Node Discovery:**
    *   By default, `antop` looks for `antnode` log files in `~/.local/share/autonomi/node/*/logs/antnode.log` to find metrics server addresses.
    *   You can override this using the `--logs` option with a glob pattern:
      ```bash
      antop --logs "/path/to/your/antnode/logs/**/*.log"
      ```

3.  **Keybindings:**
    *   `q`: Quit the application.


## Development

1.  **Setup:** Follow the "Installation from Source" steps.
2.  **Run in Debug Mode:**
    ```bash
    cargo run
    # Or with log pattern override
    cargo run -- --logs "/path/to/logs/*.log"
    ```
3.  **Run Tests:**
    ```bash
    cargo test
    ```
4.  **Linting & Formatting:**
    ```bash
    cargo fmt # Format code
    cargo clippy # Run linter
    ```

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.
