use snops_common::format::{DataFormat, DataFormatReader, DataFormatWriter, DataReadError};

pub struct PersistDrainCount {
    pub count: u32,
}

impl DataFormat for PersistDrainCount {
    type Header = u8;
    const LATEST_HEADER: Self::Header = 1;

    fn write_data<W: std::io::prelude::Write>(
        &self,
        writer: &mut W,
    ) -> Result<usize, snops_common::format::DataWriteError> {
        writer.write_data(&self.count)
    }

    fn read_data<R: std::io::prelude::Read>(
        reader: &mut R,
        header: &Self::Header,
    ) -> Result<Self, snops_common::format::DataReadError> {
        if *header != Self::LATEST_HEADER {
            return Err(DataReadError::unsupported(
                "PersistDrainCount",
                Self::LATEST_HEADER,
                header,
            ));
        }

        Ok(PersistDrainCount {
            count: reader.read_data(&())?,
        })
    }
}
