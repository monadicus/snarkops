use std::{
    fmt::Display,
    io::{Read, Write},
};

mod base_impl;
mod packed_int;

pub use packed_int::*;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DataWriteError {
    /// Error from writing data
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// A custom user defined error
    #[error("{0}")]
    Custom(String),
}

#[derive(Debug, Error)]
pub enum DataReadError {
    /// Error from reading data
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// Error from reading UTF-8 strings
    #[error("utf8 error: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),
    /// The read data cannot be automatically upgraded given the available
    /// headers
    #[error("upgrade unavailable: {0}")]
    UpgradeUnavailable(String),
    #[error("invalid version: {0}")]
    UnsupportedVersion(String),
    /// A custom user defined error
    #[error("{0}")]
    Custom(String),
}

impl DataReadError {
    pub fn unsupported(name: impl Display, expected: impl Display, found: impl Display) -> Self {
        Self::UnsupportedVersion(format!("{name}: expected {expected}, found {found}"))
    }
}

/// Write data and its header to a writer
pub fn write_dataformat<W: Write, F: DataFormat>(
    writer: &mut W,
    data: &F,
) -> Result<usize, DataWriteError> {
    Ok(data.write_header(writer)? + data.write_data(writer)?)
}

/// Read data and its header from a reader
pub fn read_dataformat<R: Read, F: DataFormat>(reader: &mut R) -> Result<F, DataReadError> {
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
        reader.read_data(&header_header)
    }

    /// Write the data to the writer
    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError>;

    /// Read the data from the reader
    fn read_data<R: Read>(reader: &mut R, header: &Self::Header) -> Result<Self, DataReadError>;

    /// Convert the data to a byte vector
    fn to_byte_vec(&self) -> Result<Vec<u8>, DataWriteError> {
        let mut buf = Vec::new();
        self.write_data(&mut buf)?;
        Ok(buf)
    }
}

pub trait DataFormatWriter {
    fn write_data<F: DataFormat>(&mut self, data: &F) -> Result<usize, DataWriteError>;
}

impl<W: Write> DataFormatWriter for W {
    fn write_data<F: DataFormat>(&mut self, data: &F) -> Result<usize, DataWriteError> {
        data.write_data(self)
    }
}

pub trait DataFormatReader {
    fn read_data<F: DataFormat>(&mut self, header: &F::Header) -> Result<F, DataReadError>;
}

impl<R: Read> DataFormatReader for R {
    fn read_data<F: DataFormat>(&mut self, header: &F::Header) -> Result<F, DataReadError> {
        F::read_data(self, header)
    }
}
