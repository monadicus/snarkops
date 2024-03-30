use indexmap::IndexMap;
use serde::Serialize;

// TODO: we could probably clean this up or make it look a little bit prettier
// later

#[derive(Debug, Clone, Serialize)]
pub struct PrometheusConfig {
    pub global: GlobalConfig,
    pub scrape_configs: Vec<ScrapeConfig>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GlobalConfig {
    pub scrape_interval: String,
    pub scrape_timeout: String,
    pub evaluation_interval: String,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            scrape_interval: "15s".into(),
            scrape_timeout: "10s".into(),
            evaluation_interval: "1m".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ScrapeConfig {
    pub job_name: String,
    pub honor_timestamps: Option<bool>,
    pub scrape_interval: Option<String>,
    pub scrape_timeout: Option<String>,
    pub metrics_path: Option<String>,
    pub scheme: Option<String>,
    pub follow_redirects: Option<bool>,
    #[serde(default)]
    pub static_configs: Vec<StaticConfig>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StaticConfig {
    pub targets: Vec<String>,
    pub labels: IndexMap<String, String>,
}
