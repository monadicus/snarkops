use std::io::{Read, Write};

use super::{DataFormat, DataReadError, DataWriteError};

macro_rules! impl_tuple_dataformat {
    ($($name:ident),+) => {
        impl<$($name: DataFormat),+> DataFormat for ($($name,)+) {
            type Header = ($($name::Header,)+);

            paste::paste! {
                fn write_data<W: Write>(&self, writer: &mut W) -> Result<(), DataWriteError>{
                    let ($([<$name:lower>],)+) = self;
                    $([<$name:lower>].write_data(writer)?;)+
                    Ok(())
                }

                fn read_data<R: Read>(header: Self::Header, reader: &mut R) -> Result<Self, DataReadError> {
                    let ($([<$name:lower>],)+) = header;
                    Ok(($($name::read_data([<$name:lower>], reader)?,)+))
                }

            }
        }
    };
}

impl_tuple_dataformat!(A, B);
impl_tuple_dataformat!(A, B, C);
impl DataFormat for () {
    type Header = ();

    fn write_data<W: Write>(&self, _writer: &mut W) -> Result<(), DataWriteError> {
        Ok(())
    }

    fn read_data<R: Read>(_header: Self::Header, _reader: &mut R) -> Result<Self, DataReadError> {
        Ok(())
    }
}

macro_rules! impl_integer_dataformat {
    ($ty:ty) => {
        impl DataFormat for $ty {
            type Header = ();

            fn write_data<W: Write>(&self, writer: &mut W) -> Result<(), DataWriteError> {
                writer.write_all(&self.to_le_bytes())?;
                Ok(())
            }

            fn read_data<R: Read>(
                _header: Self::Header,
                reader: &mut R,
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
