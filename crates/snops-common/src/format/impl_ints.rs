use std::{
    io::{Read, Write},
    num::NonZeroU8,
};

use super::{packed_int::PackedUint, DataFormat, DataReadError, DataWriteError};

macro_rules! impl_integer_dataformat {
    ($ty:ty) => {
        impl DataFormat for $ty {
            type Header = ();
            const LATEST_HEADER: Self::Header = ();

            fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
                Ok(writer.write(&self.to_le_bytes())?)
            }

            fn read_data<R: Read>(
                reader: &mut R,
                _header: &Self::Header,
            ) -> Result<Self, DataReadError> {
                let mut bytes = [0u8; core::mem::size_of::<$ty>()];
                reader.read_exact(&mut bytes)?;
                Ok(<$ty>::from_le_bytes(bytes))
            }
        }
    };
}

impl_integer_dataformat!(u8);
impl_integer_dataformat!(u16);
impl_integer_dataformat!(u32);
impl_integer_dataformat!(u64);
impl_integer_dataformat!(u128);
impl_integer_dataformat!(i8);
impl_integer_dataformat!(i16);
impl_integer_dataformat!(i32);
impl_integer_dataformat!(i64);
impl_integer_dataformat!(i128);

impl DataFormat for usize {
    type Header = ();
    const LATEST_HEADER: Self::Header = ();

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        PackedUint::from(*self).write_data(writer)
    }

    fn read_data<R: Read>(reader: &mut R, header: &Self::Header) -> Result<Self, DataReadError> {
        Ok(usize::from(PackedUint::read_data(reader, header)?))
    }
}

impl DataFormat for bool {
    type Header = ();
    const LATEST_HEADER: Self::Header = ();

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        Ok(writer.write(&[*self as u8])?)
    }

    fn read_data<R: Read>(reader: &mut R, _header: &Self::Header) -> Result<Self, DataReadError> {
        let mut byte = [0u8; 1];
        reader.read_exact(&mut byte)?;
        Ok(byte[0] != 0)
    }
}

impl DataFormat for NonZeroU8 {
    type Header = ();
    const LATEST_HEADER: Self::Header = ();

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        Ok(writer.write(&[self.get()])?)
    }

    fn read_data<R: Read>(reader: &mut R, _header: &Self::Header) -> Result<Self, DataReadError> {
        let mut byte = [0u8; 1];
        reader.read_exact(&mut byte)?;
        NonZeroU8::new(byte[0]).ok_or(DataReadError::Custom("invalid NonZeroU8".to_string()))
    }
}
