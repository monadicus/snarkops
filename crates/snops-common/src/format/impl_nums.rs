use std::{
    io::{Read, Write},
    num::NonZeroU8,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use atomic_time::{AtomicSystemTime, Ordering};

use super::{packed_int::PackedUint, DataFormat, DataFormatReader, DataReadError, DataWriteError};

macro_rules! impl_number_dataformat {
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

impl_number_dataformat!(u8);
impl_number_dataformat!(u16);
impl_number_dataformat!(u32);
impl_number_dataformat!(u64);
impl_number_dataformat!(u128);
impl_number_dataformat!(i8);
impl_number_dataformat!(i16);
impl_number_dataformat!(i32);
impl_number_dataformat!(i64);
impl_number_dataformat!(i128);
impl_number_dataformat!(f32);
impl_number_dataformat!(f64);

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

impl DataFormat for Duration {
    type Header = ();
    const LATEST_HEADER: Self::Header = ();

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        // TODO: need subsecond precision?
        let (secs, nanos) = (self.as_secs(), self.subsec_nanos());
        Ok(secs.write_data(writer)? + nanos.write_data(writer)?)
    }

    fn read_data<R: Read>(reader: &mut R, _: &Self::Header) -> Result<Self, DataReadError> {
        let (secs, nanos): (u64, u32) = (reader.read_data(&())?, reader.read_data(&())?);
        Ok(Duration::new(secs, nanos))
    }
}

impl DataFormat for SystemTime {
    type Header = ();
    const LATEST_HEADER: Self::Header = ();

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        let Ok(unix) = self.duration_since(UNIX_EPOCH) else {
            return Err(DataWriteError::custom("time is before unix epoch"));
        };

        unix.write_data(writer)
    }

    fn read_data<R: Read>(reader: &mut R, _: &Self::Header) -> Result<Self, DataReadError> {
        Ok(UNIX_EPOCH + reader.read_data::<Duration>(&())?)
    }
}

impl DataFormat for AtomicSystemTime {
    type Header = ();
    const LATEST_HEADER: Self::Header = ();

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        let time = self.load(Ordering::Acquire);
        time.write_data(writer)
    }

    fn read_data<R: Read>(reader: &mut R, _: &Self::Header) -> Result<Self, DataReadError> {
        Ok(Self::new(reader.read_data::<SystemTime>(&())?))
    }
}

#[cfg(test)]
#[rustfmt::skip]
mod test {
    use crate::format::DataFormat;

    macro_rules! case {
        ($ty:ty, $a:expr, $b:expr) => {
            paste::paste! {
                #[test]
                fn [<test_ $a>]() {
                    let mut data = Vec::new();
                    $a.write_data(&mut data).unwrap();
                    assert_eq!(data, &$b);

                    let mut reader = &data[..];
                    let read_value = $ty::read_data(&mut reader, &()).unwrap();
                    assert_eq!(read_value, $a);

                }

            }
        };
    }

    case!(u8, 0u8, [0]);
    case!(u8, 1u8, [1]);
    case!(u16, 0x1234u16, [0x34, 0x12]);
    case!(u32, 0x12345678u32, [0x78, 0x56, 0x34, 0x12]);
    case!(u64, 0x123456789abcdef0u64, [0xf0, 0xde, 0xbc, 0x9a, 0x78, 0x56, 0x34, 0x12]);
    case!(u128, 0x123456789abcdef0123456789abcdef0u128, [0xf0, 0xde, 0xbc, 0x9a, 0x78, 0x56, 0x34, 0x12, 0xf0, 0xde, 0xbc, 0x9a, 0x78, 0x56, 0x34, 0x12]);
}
