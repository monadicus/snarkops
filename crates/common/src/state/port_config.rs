use crate::format::{DataFormat, DataFormatReader};

#[derive(Debug, Copy, Clone, serde::Serialize, serde::Deserialize, clap::Parser, Eq, PartialEq)]
pub struct PortConfig {
    /// Specify the IP address and port for the node server
    #[clap(long = "node", env = "SNARKOS_PORT_NODE", default_value_t = 4130)]
    pub node: u16,

    /// Specify the IP address and port for the BFT
    #[clap(long = "bft", env = "SNARKOS_PORT_BFT", default_value_t = 5000)]
    pub bft: u16,

    /// Specify the IP address and port for the REST server
    #[clap(long = "rest", env = "SNARKOS_PORT_REST", default_value_t = 3030)]
    pub rest: u16,

    /// Specify the port for the metrics
    #[clap(long = "metrics", env = "SNARKOS_PORT_METRICS", default_value_t = 9000)]
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

#[cfg(test)]
mod test {
    use crate::format::DataFormat;
    use crate::state::PortConfig;

    macro_rules! case {
        ($name:ident, $ty:ty, $a:expr, $b:expr) => {
            #[test]
            fn $name() -> Result<(), Box<dyn std::error::Error>> {
                let mut data = Vec::new();
                $a.write_data(&mut data).unwrap();
                assert_eq!(data, $b);

                let mut reader = &data[..];
                let read_value =
                    <$ty>::read_data(&mut reader, &<$ty as DataFormat>::LATEST_HEADER).unwrap();

                // write the data again because not every type implements PartialEq
                let mut data2 = Vec::new();
                read_value.write_data(&mut data2).unwrap();
                assert_eq!(data, data2);
                Ok(())
            }
        };
    }

    case!(
        port_config,
        PortConfig,
        PortConfig {
            node: 0,
            bft: 1,
            rest: 2,
            metrics: 3,
        },
        [0, 0, 1, 0, 2, 0, 3, 0]
    );
}
