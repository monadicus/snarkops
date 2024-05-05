use crate::format::{DataFormat, DataFormatReader};

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

impl DataFormat for PortConfig {
    type Header = u8;
    const LATEST_HEADER: Self::Header = 1;

    fn write_data<W: std::io::prelude::Write>(
        &self,
        writer: &mut W,
    ) -> Result<usize, crate::format::DataWriteError> {
        let mut written = 0;
        written += self.node.write_data(writer)?;
        written += self.bft.write_data(writer)?;
        written += self.rest.write_data(writer)?;
        written += self.metrics.write_data(writer)?;
        Ok(written)
    }

    fn read_data<R: std::io::prelude::Read>(
        reader: &mut R,
        header: &Self::Header,
    ) -> Result<Self, crate::format::DataReadError> {
        if *header != Self::LATEST_HEADER {
            return Err(crate::format::DataReadError::unsupported(
                "PortConfig",
                Self::LATEST_HEADER,
                *header,
            ));
        }

        Ok(PortConfig {
            node: reader.read_data(&())?,
            bft: reader.read_data(&())?,
            rest: reader.read_data(&())?,
            metrics: reader.read_data(&())?,
        })
    }
}
