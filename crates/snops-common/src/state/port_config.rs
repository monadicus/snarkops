#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, clap::Parser)]
pub struct PortConfig {
    /// Specify the IP address and port for the node server
    #[clap(long = "node", default_value_t = 4130)]
    pub node: u16,

    /// Specify the IP address and port for the BFT
    #[clap(long = "bft", default_value_t = 5000)]
    pub bft: u16,

    /// Specify the IP address and port for the REST server
    #[clap(long = "rest", default_value_t = 3030)]
    pub rest: u16,

    /// Specify the port for the metrics
    #[clap(long = "metrics", default_value_t = 9000)]
    pub metrics: u16,
}

impl std::fmt::Display for PortConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "bft: {}, node: {}, rest: {}",
            self.bft, self.node, self.rest
        )
    }
}
