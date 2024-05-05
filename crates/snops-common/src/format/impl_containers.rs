use std::io::{Read, Write};

use super::{DataFormat, DataFormatReader, DataFormatWriter, DataReadError, DataWriteError};

impl<T: DataFormat> DataFormat for Option<T> {
    type Header = T::Header;
    const LATEST_HEADER: Self::Header = T::LATEST_HEADER;

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        Ok(match self {
            None => writer.write(&[0u8])?,
            Some(value) => writer.write(&[1u8])? + writer.write_data(value)?,
        })
    }

    fn read_data<R: Read>(reader: &mut R, header: &Self::Header) -> Result<Self, DataReadError> {
        let mut byte = [0u8; 1];
        reader.read_exact(&mut byte)?;
        Ok(match byte[0] {
            0 => None,
            1 => Some(reader.read_data(header)?),
            _ => return Err(DataReadError::Custom("invalid Option tag".to_string())),
        })
    }
}

impl<T: DataFormat> DataFormat for Box<T> {
    type Header = T::Header;
    const LATEST_HEADER: Self::Header = T::LATEST_HEADER;

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        self.as_ref().write_data(writer)
    }

    fn read_data<R: Read>(reader: &mut R, header: &Self::Header) -> Result<Self, DataReadError> {
        Ok(Box::new(reader.read_data(header)?))
    }
}
