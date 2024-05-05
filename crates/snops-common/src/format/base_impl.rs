use std::{
    io::{Read, Write},
    net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6},
    num::NonZeroU8,
};

use checkpoint::RetentionSpan;
use lasso::Spur;

use super::{
    packed_int::PackedUint, DataFormat, DataFormatReader, DataFormatWriter, DataReadError,
    DataWriteError,
};
use crate::INTERN;

macro_rules! impl_tuple_dataformat {
    ($($name:ident),+) => {
        impl<$($name: DataFormat),+> DataFormat for ($($name,)+) {
            type Header = ($($name::Header,)+);
            // TODO: potentially make these into references
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

impl DataFormat for Ipv4Addr {
    type Header = ();
    const LATEST_HEADER: Self::Header = ();

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        Ok(writer.write(&self.octets())?)
    }

    fn read_data<R: Read>(reader: &mut R, _header: &Self::Header) -> Result<Self, DataReadError> {
        let mut octets = [0u8; 4];
        reader.read_exact(&mut octets)?;
        Ok(Ipv4Addr::from(octets))
    }
}

impl DataFormat for Ipv6Addr {
    type Header = ();
    const LATEST_HEADER: Self::Header = ();

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        Ok(writer.write(&self.octets())?)
    }

    fn read_data<R: Read>(reader: &mut R, _header: &Self::Header) -> Result<Self, DataReadError> {
        let mut octets = [0u8; 16];
        reader.read_exact(&mut octets)?;
        Ok(Ipv6Addr::from(octets))
    }
}

impl DataFormat for SocketAddrV4 {
    type Header = ();
    const LATEST_HEADER: Self::Header = ();

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        Ok(self.ip().write_data(writer)? + self.port().write_data(writer)?)
    }

    fn read_data<R: Read>(reader: &mut R, header: &Self::Header) -> Result<Self, DataReadError> {
        Ok(SocketAddrV4::new(
            reader.read_data(header)?,
            reader.read_data(header)?,
        ))
    }
}

impl DataFormat for SocketAddrV6 {
    type Header = ();
    const LATEST_HEADER: Self::Header = ();

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        Ok(self.ip().write_data(writer)? + self.port().write_data(writer)?)
    }

    fn read_data<R: Read>(reader: &mut R, header: &Self::Header) -> Result<Self, DataReadError> {
        Ok(SocketAddrV6::new(
            reader.read_data(header)?,
            reader.read_data(header)?,
            0,
            0,
        ))
    }
}

impl DataFormat for SocketAddr {
    type Header = ();
    const LATEST_HEADER: Self::Header = ();

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        match self {
            SocketAddr::V4(addr) => Ok(0u8.write_data(writer)? + addr.write_data(writer)?),
            SocketAddr::V6(addr) => Ok(1u8.write_data(writer)? + addr.write_data(writer)?),
        }
    }

    fn read_data<R: Read>(reader: &mut R, _header: &Self::Header) -> Result<Self, DataReadError> {
        match reader.read_data(&())? {
            0u8 => Ok(SocketAddr::V4(reader.read_data(&())?)),
            1u8 => Ok(SocketAddr::V6(reader.read_data(&())?)),
            n => Err(DataReadError::Custom(format!(
                "invalid SocketAddr discriminant: {n}"
            ))),
        }
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

impl DataFormat for RetentionSpan {
    type Header = u8;
    const LATEST_HEADER: Self::Header = 1;

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        match self {
            RetentionSpan::Unlimited => 0u8.write_data(writer),
            RetentionSpan::Minute(b) => {
                1u8.write_data(writer)?;
                b.write_data(writer)
            }
            RetentionSpan::Hour(b) => {
                2u8.write_data(writer)?;
                b.write_data(writer)
            }
            RetentionSpan::Day(b) => {
                3u8.write_data(writer)?;
                b.write_data(writer)
            }
            RetentionSpan::Week(b) => {
                4u8.write_data(writer)?;
                b.write_data(writer)
            }
            RetentionSpan::Month(b) => {
                5u8.write_data(writer)?;
                b.write_data(writer)
            }
            RetentionSpan::Year(b) => {
                6u8.write_data(writer)?;
                b.write_data(writer)
            }
        }
    }

    fn read_data<R: Read>(reader: &mut R, header: &Self::Header) -> Result<Self, DataReadError> {
        if *header != Self::LATEST_HEADER {
            return Err(DataReadError::unsupported(
                "RetentionSpan",
                Self::LATEST_HEADER,
                *header,
            ));
        }
        match reader.read_data(&())? {
            0u8 => Ok(RetentionSpan::Unlimited),
            1u8 => Ok(RetentionSpan::Minute(reader.read_data(&())?)),
            2u8 => Ok(RetentionSpan::Hour(reader.read_data(&())?)),
            3u8 => Ok(RetentionSpan::Day(reader.read_data(&())?)),
            4u8 => Ok(RetentionSpan::Week(reader.read_data(&())?)),
            5u8 => Ok(RetentionSpan::Month(reader.read_data(&())?)),
            6u8 => Ok(RetentionSpan::Year(reader.read_data(&())?)),
            n => Err(DataReadError::Custom(format!(
                "invalid RetentionSpan discrminant: {n}",
            ))),
        }
    }
}
