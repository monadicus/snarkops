use std::io::{Read, Write};

use lasso::Spur;

use super::{packed_int::PackedUint, DataFormat, DataReadError, DataWriteError};
use crate::INTERN;

impl DataFormat for String {
    type Header = ();
    const LATEST_HEADER: Self::Header = ();

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        let bytes = self.as_bytes();
        Ok(PackedUint::from(bytes.len()).write_data(writer)? + writer.write(bytes)?)
    }

    fn read_data<R: Read>(reader: &mut R, _header: &Self::Header) -> Result<Self, DataReadError> {
        let len = usize::from(PackedUint::read_data(reader, &())?);
        let mut buf = String::with_capacity(len);
        let read_len = reader.take(len as u64).read_to_string(&mut buf)?;
        if read_len != len {
            return Err(DataReadError::Custom(format!(
                "string expected to read {} bytes, but read {}",
                len, read_len
            )));
        }
        Ok(buf)
    }
}

impl DataFormat for Spur {
    type Header = ();
    const LATEST_HEADER: Self::Header = ();

    fn write_data<W: std::io::prelude::Write>(
        &self,
        writer: &mut W,
    ) -> Result<usize, crate::format::DataWriteError> {
        let s: &str = INTERN.resolve(self);
        let bytes = s.as_bytes();
        Ok(PackedUint::from(bytes.len()).write_data(writer)? + writer.write(bytes)?)
    }

    fn read_data<R: std::io::prelude::Read>(
        reader: &mut R,
        _header: &Self::Header,
    ) -> Result<Self, crate::format::DataReadError> {
        let data = String::read_data(reader, &())?;
        Ok(INTERN.get_or_intern(data))
    }
}
