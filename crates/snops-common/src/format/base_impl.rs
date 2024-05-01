use std::io::{Read, Write};

use super::{
    packed_int::PackedUint, DataFormat, DataFormatReader, DataFormatWriter, DataReadError,
    DataWriteError,
};

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

                fn read_data<R: Read>(reader: &mut R, header: &Self::Header) -> Result<Self, DataReadError> {
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

    fn read_data<R: Read>(_reader: &mut R, _header: &Self::Header) -> Result<Self, DataReadError> {
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

impl DataFormat for String {
    type Header = ();
    const LATEST_HEADER: Self::Header = ();

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        let bytes = self.as_bytes();
        Ok(PackedUint::from(bytes.len()).write_data(writer)? + writer.write(bytes)?)
    }

    fn read_data<R: Read>(reader: &mut R, _header: &Self::Header) -> Result<Self, DataReadError> {
        let len = usize::from(PackedUint::read_data(reader, &())?);
        let mut bytes = vec![0u8; len];
        reader.read_exact(&mut bytes)?;
        Ok(String::from_utf8(bytes)?)
    }
}

impl<T: DataFormat + Default + Copy, const N: usize> DataFormat for [T; N] {
    type Header = T::Header;
    const LATEST_HEADER: Self::Header = T::LATEST_HEADER;

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        let mut written = 0;
        for item in self.iter() {
            written += item.write_data(writer)?;
        }
        Ok(written)
    }

    fn read_data<R: Read>(reader: &mut R, header: &Self::Header) -> Result<Self, DataReadError> {
        let mut data = [T::default(); N];
        for item in data.iter_mut() {
            *item = reader.read_data(header)?;
        }
        Ok(data)
    }
}

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

impl<T: DataFormat> DataFormat for Vec<T> {
    type Header = T::Header;
    const LATEST_HEADER: Self::Header = T::LATEST_HEADER;

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        let mut written = PackedUint::from(self.len()).write_data(writer)?;
        for item in self.iter() {
            written += writer.write_data(item)?;
        }
        Ok(written)
    }

    fn read_data<R: Read>(reader: &mut R, header: &Self::Header) -> Result<Self, DataReadError> {
        let len = usize::from(PackedUint::read_data(reader, &())?);
        let mut data = Vec::with_capacity(len);
        for _ in 0..len {
            data.push(reader.read_data(header)?);
        }
        Ok(data)
    }
}
