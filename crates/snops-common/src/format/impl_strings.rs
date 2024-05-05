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

#[cfg(test)]
#[rustfmt::skip]
mod test {
    use lasso::Spur;

    use crate::{format::DataFormat, INTERN};

    macro_rules! case {
        ($name:ident, $ty:ty, $a:expr, $b:expr) => {
            #[test]
            fn $name() {
                let mut data = Vec::new();
                $a.write_data(&mut data).unwrap();
                assert_eq!(data, $b);

                let mut reader = &data[..];
                let read_value = <$ty>::read_data(&mut reader, &()).unwrap();
                assert_eq!(read_value, $a);

            }

        };
    }

    case!(test_string, String, "hello".to_string(), b"\x01\x05hello");
    case!(test_spur, Spur, INTERN.get_or_intern("hello"), b"\x01\x05hello");
    // 0x15 is 21, which is the length of the string
    case!(test_long_string, String, "This is a long string".to_string(), b"\x01\x15This is a long string");
    // 0x1A is 26, which is the length of the string
    case!(test_interned_string, Spur, INTERN.get_or_intern("This is an interned string"), b"\x01\x1AThis is an interned string");
    case!(test_empty_string, String, "".to_string(), [1, 0]);
    case!(test_empty_spur, Spur, INTERN.get_or_intern(""), [1, 0]);
}
