use simple_moving_average::{SMA, SingleSumSMA};

use super::*;

// TODO: this is 5 minutes in the grafana
pub const TPS_WINDOW: Duration = Duration::from_secs(10); // `from_mins` is unstable for some reason???
pub const TPS_WINDOW_LEN: usize = (TPS_WINDOW.as_secs() / UPDATE_RATE.as_secs()) as usize;
pub const TPS_METRIC: &str = "snarkos_blocks_transactions_total";

pub struct TpsMetric {
    last: Option<f64>,
    sma: SingleSumSMA<f64, f64, TPS_WINDOW_LEN>,
}

impl Default for TpsMetric {
    fn default() -> Self {
        Self {
            last: Default::default(),
            sma: SingleSumSMA::new(),
        }
    }
}

impl MetricComputer for TpsMetric {
    fn update(&mut self, metrics: &ParsedMetrics<'_>) {
        match (metrics.get(TPS_METRIC).copied(), self.last) {
            // no `last` metric is set
            // TODO: need a good way not to add a MASSIVE rate when we first start metrics again
            (Some(cur), None) => self.last = Some(cur),

            // a last metric is set
            (Some(cur), Some(ref mut prev)) => {
                self.sma.add_sample(cur - *prev);
                *prev = cur;
            }

            // by default, add a zero rate
            _ => {
                self.sma.add_sample(0.0);
            }
        }
    }

    fn get(&self) -> f64 {
        self.sma.get_average() / UPDATE_RATE.as_secs_f64()
    }
}
