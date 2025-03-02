use std::io::{Read, Write};

use super::{DataFormat, DataReadError, DataWriteError};

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

#[cfg(test)]
#[rustfmt::skip]
mod test {
    use crate::format::DataFormat;

    macro_rules! case {
        ($name:ident, $ty:ty, $a:expr_2021, $b:expr_2021) => {
            #[test]
            fn $name() {
                let mut data = Vec::new();
                $a.write_data(&mut data).unwrap();
                assert_eq!(data, &$b);

                let mut reader = &data[..];
                let read_value = <$ty>::read_data(&mut reader, &<$ty as DataFormat>::LATEST_HEADER).unwrap();
                assert_eq!(read_value, $a);

            }

        };
    }

    case!(test_tuple_0, (), (), [0u8; 0]);
    case!(test_tuple_2, (u8, u16), (1u8, 2u16), [1, 2, 0]);
    case!(test_tuple_3, (u8, u16, u32), (1u8, 2u16, 3u32), [
        1,
        2, 0,
        3, 0, 0, 0
    ]);
    case!(test_tuple_2_1, ((u8, u16), u32), ((1u8, 2u16), 3u32), [
        1,
        2, 0,
        3, 0, 0, 0
    ]);
    case!(test_tuple_2_2, ((u8, u16), (u32, u64)), ((1u8, 2u16), (3u32, 4u64)), [
        1,
        2, 0,
        3, 0, 0, 0,
        4, 0, 0, 0, 0, 0, 0, 0
    ]);
    case!(test_tuple_2_rev, (u16, u8), (2u16, 1u8), [2, 0, 1]);
    case!(test_tuple_2_1_rev, (u32, (u16, u8)), (3u32, (2u16, 1u8)), [
        3, 0, 0, 0,
        2, 0,
        1
    ]);
    case!(test_tuple_2_2_rev, ((u32, u64), (u16, u8)), ((3u32, 4u64), (2u16, 1u8)), [
        3, 0, 0, 0,
        4, 0, 0, 0, 0, 0, 0, 0,
        2,
        0, 1
    ]);

}
