use std::io::{Read, Write};

mod base_impl;

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
    #[error("{0}")]
    Custom(String),
}

pub fn write_data_with_headers<W: Write, F: DataFormat>(
    writer: &mut W,
    data: &F,
) -> Result<(), DataWriteError> {
    // write the default (latest) header
    F::Header::default().write_data(writer)?;
    // write the data
    data.write_data(writer)
}

pub fn read_data<R: Read, F: DataFormat>(reader: &mut R) -> Result<F, DataReadError> {
    // the header is read with a default version as headers cannot have versioned
    // headers...
    let header_header = <F::Header as DataFormat>::Header::default();
    let header = F::Header::read_data(header_header, reader)?;

    // using the header, read the data
    F::read_data(header, reader)
}

/// `DataFormat` is a trait for serializing and deserializing binary data.
///
/// A header is read/written containing the versions of the desired data
pub trait DataFormat: Sized {
    type Header: DataFormat + Clone + Copy + Default + Sized;

    /// Write the data to the writer
    fn write_data<W: Write>(&self, writer: &mut W) -> Result<(), DataWriteError>;

    /// Read the data from the reader
    fn read_data<R: Read>(header: Self::Header, reader: &mut R) -> Result<Self, DataReadError>;
}
