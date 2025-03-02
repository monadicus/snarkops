use std::io::{Read, Write};

use super::{DataFormat, DataReadError, DataWriteError};

pub struct PackedUint(pub u64);

impl From<PackedUint> for usize {
    fn from(value: PackedUint) -> Self {
        value.0 as usize
    }
}

impl From<usize> for PackedUint {
    fn from(value: usize) -> Self {
        PackedUint(value as u64)
    }
}

impl DataFormat for PackedUint {
    type Header = ();
    const LATEST_HEADER: Self::Header = ();

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        // count number of leading zeroes and calculate the number of bytes needed
        // to store the non-zero bits
        let zeroes = self.0.leading_zeros();
        let num_bytes = 8u8.saturating_sub((zeroes / 8) as u8);

        // write the number of bytes and the bytes themselves
        let bytes = &self.0.to_le_bytes()[..num_bytes as usize];
        Ok(writer.write(&[num_bytes])? + writer.write(bytes)?)
    }

    fn read_data<R: Read>(reader: &mut R, _header: &Self::Header) -> Result<Self, DataReadError> {
        // read the number of bytes to read
        let mut num_bytes = [0u8; 1];
        reader.read_exact(&mut num_bytes)?;

        if num_bytes[0] > 8 {
            return Err(DataReadError::Custom(format!(
                "PackedUint received `{}` length, which is greater than 8",
                num_bytes[0]
            )));
        }

        // read that many bytes
        let mut bytes = [0u8; 8];
        reader.read_exact(&mut bytes[..num_bytes[0] as usize])?;

        // convert the LE bytes to a u64
        Ok(PackedUint(u64::from_le_bytes(bytes)))
    }
}

#[cfg(test)]
#[rustfmt::skip]
#[allow(clippy::unusual_byte_groupings)]
mod test {
    use super::*;

    macro_rules! case {
        ($a:expr_2021, $b:expr_2021) => {
            paste::paste! {
                #[test]
                fn [<test_ $a>]() {
                    let mut data = Vec::new();
                    let value = PackedUint($a);
                    value.write_data(&mut data).unwrap();
                    assert_eq!(data, &$b);

                    let mut reader = &data[..];
                    let read_value = PackedUint::read_data(&mut reader, &()).unwrap();
                    assert_eq!(read_value.0, value.0);
                }

            }
        };
    }
    case!(0x12345678, [4, 0x78, 0x56, 0x34, 0x12]);

    case!(0, [0]);
    case!(0xff, [1, 0xff]);
    case!(0xffff, [2, 0xff, 0xff]);
    case!(0xffff_ff, [3, 0xff, 0xff, 0xff]);
    case!(0xffff_ffff, [4, 0xff, 0xff, 0xff, 0xff]);
    case!(0xffff_ffff_ff, [5, 0xff, 0xff, 0xff, 0xff, 0xff]);
    case!(0xffff_ffff_ffff, [6, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]);
    case!(0xffff_ffff_ffff_ff, [7, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]);
    case!(0xffff_ffff_ffff_ffff, [8, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]);

    case!(0x1, [1, 0x1]);
    case!(0x10, [1, 0x10]);
    case!(0x100, [2, 0, 0x1]);
    case!(0x1000, [2, 0, 0x10]);
    case!(0x10000, [3, 0, 0, 0x1]);
    case!(0x100000, [3, 0, 0, 0x10]);
    case!(0x1000000, [4, 0, 0, 0, 0x1]);
    case!(0x10000000, [4, 0, 0, 0, 0x10]);
    case!(0x100000000, [5, 0, 0, 0, 0, 0x1]);
    case!(0x1000000000, [5, 0, 0, 0, 0, 0x10]);
    case!(0x10000000000, [6, 0, 0, 0, 0, 0, 0x1]);
    case!(0x100000000000, [6, 0, 0, 0, 0, 0, 0x10]);
    case!(0x1000000000000, [7, 0, 0, 0, 0, 0, 0, 0x1]);
    case!(0x10000000000000, [7, 0, 0, 0, 0, 0, 0, 0x10]);
    case!(0x100000000000000, [8, 0, 0, 0, 0, 0, 0, 0, 0x1]);
    case!(0x1000000000000000, [8, 0, 0, 0, 0, 0, 0, 0, 0x10]);


    case!(0x11, [1, 0x11]);
    case!(0x101, [2, 1, 0x1]);
    case!(0x1001, [2, 1, 0x10]);
    case!(0x10001, [3, 1, 0, 0x1]);
    case!(0x100001, [3, 1, 0, 0x10]);
    case!(0x1000001, [4, 1, 0, 0, 0x1]);
    case!(0x10000001, [4, 1, 0, 0, 0x10]);
    case!(0x100000001, [5, 1, 0, 0, 0, 0x1]);
    case!(0x1000000001, [5, 1, 0, 0, 0, 0x10]);
    case!(0x10000000001, [6, 1, 0, 0, 0, 0, 0x1]);
    case!(0x100000000001, [6, 1, 0, 0, 0, 0, 0x10]);
    case!(0x1000000000001, [7, 1, 0, 0, 0, 0, 0, 0x1]);
    case!(0x10000000000001, [7, 1, 0, 0, 0, 0, 0, 0x10]);
    case!(0x100000000000001, [8, 1, 0, 0, 0, 0, 0, 0, 0x1]);
    case!(0x1000000000000001, [8, 1, 0, 0, 0, 0, 0, 0, 0x10]);
}
