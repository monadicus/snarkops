use std::io::{Read, Write};

use super::{DataFormat, DataReadError, DataWriteError};

macro_rules! impl_tuple_dataformat {
    ($($name:ident),+) => {
        impl<$($name: DataFormat),+> DataFormat for ($($name,)+) {
            type Header = ($($name::Header,)+);
            const LATEST_HEADER: Self::Header = ($($name::LATEST_HEADER,)+);

            paste::paste! {
                fn write_header<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
                    let ($([<$name:lower>],)+) = self;
                    let mut written = 0;
                    $(written += [<$name:lower>].write_header(writer)?;)+
                    Ok(written)
                }

                fn read_header<R: Read>(reader: &mut R) -> Result<Self::Header, DataReadError> {
                    Ok(($($name::read_header(reader)?,)+))
                }

                fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError>{
                    let ($([<$name:lower>],)+) = self;
                    let mut written = 0;
                    $(written += [<$name:lower>].write_data(writer)?;)+
                    Ok(written)
                }

                fn read_data<R: Read>(reader: &mut R, header: Self::Header) -> Result<Self, DataReadError> {
                    let ($([<$name:lower>],)+) = header;
                    Ok(($($name::read_data(reader, [<$name:lower>])?,)+))
                }

            }
        }
    };
}

impl_tuple_dataformat!(A, B);
impl_tuple_dataformat!(A, B, C);
impl DataFormat for () {
    type Header = ();
    const LATEST_HEADER: Self::Header = ();

    fn write_header<W: Write>(&self, _writer: &mut W) -> Result<usize, DataWriteError> {
        Ok(0)
    }

    fn read_header<R: Read>(_reader: &mut R) -> Result<Self::Header, DataReadError> {
        Ok(())
    }

    fn write_data<W: Write>(&self, _writer: &mut W) -> Result<usize, DataWriteError> {
        Ok(0)
    }

    fn read_data<R: Read>(_reader: &mut R, _header: Self::Header) -> Result<Self, DataReadError> {
        Ok(())
    }
}

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
                _header: Self::Header,
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

pub enum DynamicLength {
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
}

impl DataFormat for DynamicLength {
    type Header = ();
    const LATEST_HEADER: Self::Header = ();

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        Ok(match self {
            DynamicLength::U8(val) => 0u8.write_data(writer)? + val.write_data(writer)?,
            DynamicLength::U16(val) => 1u8.write_data(writer)? + val.write_data(writer)?,
            DynamicLength::U32(val) => 2u8.write_data(writer)? + val.write_data(writer)?,
            DynamicLength::U64(val) => 3u8.write_data(writer)? + val.write_data(writer)?,
        })
    }

    fn read_data<R: Read>(reader: &mut R, _header: Self::Header) -> Result<Self, DataReadError> {
        let mut byte = [0u8; 1];
        reader.read_exact(&mut byte)?;
        let byte = byte[0];
        Ok(match byte {
            0 => DynamicLength::U8(u8::read_data(reader, ())?),
            1 => DynamicLength::U16(u16::read_data(reader, ())?),
            2 => DynamicLength::U32(u32::read_data(reader, ())?),
            3 => DynamicLength::U64(u64::read_data(reader, ())?),
            _ => {
                return Err(DataReadError::Custom(
                    "Invalid DynamicLength byte".to_string(),
                ))
            }
        })
    }
}

impl From<usize> for DynamicLength {
    fn from(val: usize) -> Self {
        if val <= u8::MAX as usize {
            DynamicLength::U8(val as u8)
        } else if val <= u16::MAX as usize {
            DynamicLength::U16(val as u16)
        } else if val <= u32::MAX as usize {
            DynamicLength::U32(val as u32)
        } else {
            DynamicLength::U64(val as u64)
        }
    }
}

impl From<DynamicLength> for usize {
    fn from(val: DynamicLength) -> Self {
        match val {
            DynamicLength::U8(val) => val as usize,
            DynamicLength::U16(val) => val as usize,
            DynamicLength::U32(val) => val as usize,
            DynamicLength::U64(val) => val as usize,
        }
    }
}

impl DataFormat for String {
    type Header = ();
    const LATEST_HEADER: Self::Header = ();

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        let bytes = self.as_bytes();
        let mut written = 0;
        written += DynamicLength::from(bytes.len()).write_data(writer)?;
        written += writer.write(bytes)?;
        Ok(written)
    }

    fn read_data<R: Read>(reader: &mut R, _header: Self::Header) -> Result<Self, DataReadError> {
        let len = DynamicLength::read_data(reader, ())?;
        let mut bytes = vec![0u8; usize::from(len)];
        reader.read_exact(&mut bytes)?;
        String::from_utf8(bytes).map_err(|_| DataReadError::Custom("Invalid UTF-8".to_string()))
    }
}
