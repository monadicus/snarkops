use super::prelude::*;

pub struct PersistDrainCount {
    pub count: u32,
}

impl DataFormat for PersistDrainCount {
    type Header = u8;
    const LATEST_HEADER: Self::Header = 1;

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        writer.write_data(&self.count)
    }

    fn read_data<R: Read>(reader: &mut R, header: &Self::Header) -> Result<Self, DataReadError> {
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
