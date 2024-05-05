use std::io::{Read, Write};

use checkpoint::{RetentionPolicy, RetentionRule, RetentionSpan};

use super::{DataFormat, DataFormatReader, DataReadError, DataWriteError};

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

impl DataFormat for RetentionPolicy {
    type Header = (u8, <RetentionSpan as DataFormat>::Header);

    const LATEST_HEADER: Self::Header = (1, RetentionSpan::LATEST_HEADER);

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        let rules = self
            .rules
            .iter()
            .map(|r| (r.duration, r.keep))
            .collect::<Vec<_>>();
        rules.write_data(writer)
    }

    fn read_data<R: Read>(reader: &mut R, header: &Self::Header) -> Result<Self, DataReadError> {
        if header.0 != Self::LATEST_HEADER.0 {
            return Err(DataReadError::unsupported(
                "RetentionPolicy",
                Self::LATEST_HEADER.0,
                header.0,
            ));
        }

        let rules =
            Vec::<(RetentionSpan, RetentionSpan)>::read_data(reader, &(header.1, header.1))?;
        Ok(RetentionPolicy {
            rules: rules
                .into_iter()
                .map(|(duration, keep)| RetentionRule { duration, keep })
                .collect(),
        })
    }
}
