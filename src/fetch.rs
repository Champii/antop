use anyhow::Result; // Keep Result for potential internal errors, though return type is specific
use futures::future::join_all;
use reqwest;
use std::time::Duration;

/// Fetches metrics data from a list of server addresses concurrently.
/// Returns a vector of tuples: (address, Result<raw_metrics_string, error_string>).
pub async fn fetch_metrics(
    addresses: &[String],
) -> Vec<(String, Result<String, String>)> { // Using Result<String, String> as per original design
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2)) // Shorter timeout for TUI responsiveness
        .build()
        // Consider proper error handling instead of unwrap_or_else
        .unwrap_or_else(|_| reqwest::Client::new());

    let futures = addresses.iter().map(|addr| {
        let client = client.clone();
        let addr = addr.clone();
        async move {
            let url = format!("{}/metrics", addr);
            let result = client.get(&url).send().await;

            match result {
                Ok(response) => match response.error_for_status() {
                    Ok(successful_response) => match successful_response.text().await {
                        Ok(text) => (addr, Ok(text)),
                        Err(e) => (addr, Err(format!("Read body error: {}", e))),
                    },
                    Err(status_error) => (addr, Err(format!("HTTP error: {}", status_error))),
                },
                Err(network_error) => (addr, Err(format!("Network error: {}", network_error))),
            }
        }
    });

    join_all(futures).await
}