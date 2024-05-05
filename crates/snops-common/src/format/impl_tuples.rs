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
