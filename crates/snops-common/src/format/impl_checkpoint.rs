use std::io::{Read, Write};

use snops_checkpoint::{RetentionPolicy, RetentionRule, RetentionSpan};

use super::{DataFormat, DataFormatReader, DataHeaderOf, DataReadError, DataWriteError};

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
    type Header = (u8, DataHeaderOf<RetentionSpan>);

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

#[cfg(test)]
#[rustfmt::skip]
mod test {
    use crate::format::DataFormat;
    use snops_checkpoint::{RetentionPolicy, RetentionSpan};


    macro_rules! case {
        ($name:ident, $ty:ty, $a:expr, $b:expr) => {
            #[test]
            fn $name() {
                let mut data = Vec::new();
                let value: $ty = $a.parse().unwrap();
                value.write_data(&mut data).unwrap();
                assert_eq!(data, $b);

                let mut reader = &data[..];
                let read_value = <$ty>::read_data(&mut reader, &<$ty as DataFormat>::LATEST_HEADER).unwrap();
                assert_eq!(read_value, value);

            }

        };
    }

    case!(retention_span_unlimited, RetentionSpan, "U", [0]);
    case!(retention_span_minute, RetentionSpan, "1m", [1, 1]);
    case!(retention_span_hour, RetentionSpan, "1h", [2, 1]);
    case!(retention_span_day, RetentionSpan, "1D", [3, 1]);
    case!(retention_span_week, RetentionSpan, "1W", [4, 1]);
    case!(retention_span_month, RetentionSpan, "1M", [5, 1]);
    case!(retention_span_year, RetentionSpan, "1Y", [6, 1]);

    case!(retention_policy, RetentionPolicy, "1m:1m,1h:1h,1D:1D,1W:1W,1M:1M,1Y:1Y", [
        1, 6,
        1, 1, 1, 1,
        2, 1, 2, 1,
        3, 1, 3, 1,
        4, 1, 4, 1,
        5, 1, 5, 1,
        6, 1, 6, 1
    ]);

    case!(retention_policy_u_u, RetentionPolicy, "U:U", [
        1, 1,
        0, 0
    ]);

    case!(retention_policy_u_1y, RetentionPolicy, "U:1Y", [
        1, 1,
        0, 6, 1
    ]);
}
