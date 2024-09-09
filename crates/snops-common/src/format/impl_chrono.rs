use chrono::{DateTime, Utc};

use super::{DataFormat, DataReadError};

impl DataFormat for DateTime<Utc> {
    type Header = ();
    const LATEST_HEADER: Self::Header = ();

    fn write_data<W: std::io::Write>(
        &self,
        writer: &mut W,
    ) -> Result<usize, super::DataWriteError> {
        Ok(self.timestamp().write_data(writer)?
            + self.timestamp_subsec_nanos().write_data(writer)?)
    }

    fn read_data<R: std::io::Read>(
        reader: &mut R,
        _header: &Self::Header,
    ) -> Result<Self, super::DataReadError> {
        let timestamp = i64::read_data(reader, &())?;
        let subsec_nanos = u32::read_data(reader, &())?;
        DateTime::from_timestamp(timestamp, subsec_nanos).ok_or_else(|| {
            DataReadError::custom(format!(
                "Invalid timestamp in datetime: {timestamp}.{subsec_nanos}"
            ))
        })
    }
}
