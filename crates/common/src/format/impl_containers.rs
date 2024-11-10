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

#[cfg(test)]
#[rustfmt::skip]
mod test {
    use crate::format::DataFormat;

    macro_rules! case {
        ($name:ident, $ty:ty, $a:expr, $b:expr) => {
            #[test]
            fn $name() {
                let mut data = Vec::new();
                let value: $ty = $a;
                value.write_data(&mut data).unwrap();
                assert_eq!(data, &$b);

                let mut reader = &data[..];
                let read_value = <$ty>::read_data(&mut reader, &<$ty as DataFormat>::LATEST_HEADER).unwrap();
                assert_eq!(read_value, value);

            }

        };
    }

    case!(test_option_none, Option<u8>, None, [0]);
    case!(test_option_some, Option<u8>, Some(1), [1, 1]);
}
