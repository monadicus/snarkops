pub mod tps;

use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};

use self::tps::TpsMetric;
use crate::state::GlobalState;

pub const UPDATE_RATE: Duration = Duration::from_secs(15);

#[derive(Default)]
pub struct Metrics {
    pub tps: TpsMetric,
}

/// Parsed metrics from the snarkOS Prometheus scraper.
type ParsedMetrics<'a> = HashMap<&'a str, f64>;

pub fn init(state: Arc<GlobalState>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(UPDATE_RATE);
        let client = reqwest::Client::new();

        loop {
            interval.tick().await;

            // TODO: this could probably be improved, but we want to avoid scraping metrics
            // if the child doesn't exist
            if state.child.read().await.is_none() {
                continue;
            }

            // TODO: maybe this should use bind_addr
            let metrics_text = 'metrics: {
                let response = match client
                    .get(format!(
                        "http://{}/",
                        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), state.cli.ports.metrics)
                    ))
                    .send()
                    .await
                {
                    Ok(response) => response,
                    Err(_e) => {
                        break 'metrics Default::default();
                    }
                };

                response.text().await.unwrap_or_default()
            };

            let metrics = parse_metrics(&metrics_text);

            let mut metrics_lock = state.metrics.write().await;
            metrics_lock.tps.update(&metrics);

            // TODO: TPS, # of blocks behind
        }
    });
}

/// Parse the metrics blob when scraping the snarkOS Prometheus exporter.
fn parse_metrics(source: &str) -> ParsedMetrics<'_> {
    source
        .split('\n')
        .map(str::trim)
        .filter_map(|line| {
            if line.is_empty() || line.starts_with('#') {
                return None;
            }

            let (key, value) = line.split_once(' ')?;
            let value = value.parse().ok()?;

            Some((key, value))
        })
        .collect()
}

pub trait MetricComputer: Default {
    fn update(&mut self, metrics: &ParsedMetrics<'_>);
    fn get(&self) -> f64;
}
