use std::io::{Read, Write};

mod base_impl;
mod packed_int;

pub use packed_int::*;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DataWriteError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Custom(String),
}

#[derive(Debug, Error)]
pub enum DataReadError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("utf8 error: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),
    #[error("{0}")]
    Custom(String),
}

pub fn write_data<W: Write, F: DataFormat>(
    writer: &mut W,
    data: &F,
) -> Result<usize, DataWriteError> {
    Ok(data.write_header(writer)? + data.write_data(writer)?)
}

pub fn read_data<R: Read, F: DataFormat>(reader: &mut R) -> Result<F, DataReadError> {
    let header = F::read_header(reader)?;
    F::read_data(reader, &header)
}

/// `DataFormat` is a trait for serializing and deserializing binary data.
///
/// A header is read/written containing the versions of the desired data
pub trait DataFormat: Sized {
    type Header: DataFormat + Clone + Sized;
    const LATEST_HEADER: Self::Header;

    fn write_header<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        Ok(Self::LATEST_HEADER.write_header(writer)? + Self::LATEST_HEADER.write_data(writer)?)
    }

    fn read_header<R: Read>(reader: &mut R) -> Result<Self::Header, DataReadError> {
        // read the header's header
        let header_header = Self::Header::read_header(reader)?;
        // read the header's data
        let header = Self::Header::read_data(reader, &header_header)?;
        Ok(header)
    }

    /// Write the data to the writer
    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError>;

    /// Read the data from the reader
    fn read_data<R: Read>(reader: &mut R, header: &Self::Header) -> Result<Self, DataReadError>;
}
