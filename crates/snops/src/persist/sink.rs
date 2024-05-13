use snops_common::format::{
    read_dataformat, write_dataformat, DataFormat, DataFormatReader, DataHeaderOf,
};

use crate::{
    cannon::sink::{FireRate, TxSink},
    schema::NodeTargets,
};

#[derive(Debug, Clone)]
pub struct TxSinkFormatHeader {
    pub version: u8,
    pub node_targets: DataHeaderOf<NodeTargets>,
    pub fire_rate: DataHeaderOf<FireRate>,
}

impl DataFormat for TxSinkFormatHeader {
    type Header = u8;
    const LATEST_HEADER: Self::Header = 1;

    fn write_data<W: std::io::prelude::Write>(
        &self,
        writer: &mut W,
    ) -> Result<usize, snops_common::format::DataWriteError> {
        Ok(self.version.write_data(writer)?
            + write_dataformat(writer, &self.node_targets)?
            + self.fire_rate.write_data(writer)?)
    }

    fn read_data<R: std::io::prelude::Read>(
        reader: &mut R,
        header: &Self::Header,
    ) -> Result<Self, snops_common::format::DataReadError> {
        if *header != Self::LATEST_HEADER {
            return Err(snops_common::format::DataReadError::unsupported(
                "TxSinkFormatHeader",
                Self::LATEST_HEADER,
                *header,
            ));
        }

        let version = reader.read_data(&())?;
        let node_targets = read_dataformat(reader)?;
        let fire_rate = reader.read_data(&())?;
        Ok(Self {
            version,
            node_targets,
            fire_rate,
        })
    }
}

impl DataFormat for TxSink {
    type Header = TxSinkFormatHeader;
    const LATEST_HEADER: Self::Header = TxSinkFormatHeader {
        version: 1,
        node_targets: NodeTargets::LATEST_HEADER,
        fire_rate: FireRate::LATEST_HEADER,
    };

    fn write_data<W: std::io::prelude::Write>(
        &self,
        writer: &mut W,
    ) -> Result<usize, snops_common::format::DataWriteError> {
        let mut written = 0;
        match self {
            TxSink::Record {
                file_name,
                tx_request_delay_ms,
            } => {
                written += 0u8.write_data(writer)?;
                written += file_name.write_data(writer)?;
                written += tx_request_delay_ms.write_data(writer)?;
            }
            TxSink::RealTime { target, rate } => {
                written += 1u8.write_data(writer)?;
                written += target.write_data(writer)?;
                written += rate.write_data(writer)?;
            }
        }

        Ok(written)
    }

    fn read_data<R: std::io::prelude::Read>(
        reader: &mut R,
        header: &Self::Header,
    ) -> Result<Self, snops_common::format::DataReadError> {
        if header.version != Self::LATEST_HEADER.version {
            return Err(snops_common::format::DataReadError::unsupported(
                "TxSink",
                Self::LATEST_HEADER.version,
                header.version,
            ));
        }

        match reader.read_data(&())? {
            0u8 => {
                let file_name = reader.read_data(&())?;
                let tx_request_delay_ms = reader.read_data(&())?;
                Ok(TxSink::Record {
                    file_name,
                    tx_request_delay_ms,
                })
            }
            1u8 => {
                let target = reader.read_data(&header.node_targets)?;
                let rate = reader.read_data(&header.fire_rate)?;
                Ok(TxSink::RealTime { target, rate })
            }
            n => Err(snops_common::format::DataReadError::Custom(format!(
                "invalid TxSink discriminant: {n}"
            ))),
        }
    }
}
