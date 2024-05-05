use std::{
    collections::{HashMap, HashSet},
    io::{Read, Write},
};

use super::{
    packed_int::PackedUint, DataFormat, DataFormatReader, DataFormatWriter, DataReadError,
    DataWriteError,
};

impl<T: DataFormat + Default + Copy, const N: usize> DataFormat for [T; N] {
    type Header = T::Header;
    const LATEST_HEADER: Self::Header = T::LATEST_HEADER;

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        let mut written = 0;
        for item in self.iter() {
            written += item.write_data(writer)?;
        }
        Ok(written)
    }

    fn read_data<R: Read>(reader: &mut R, header: &Self::Header) -> Result<Self, DataReadError> {
        let mut data = [T::default(); N];
        for item in data.iter_mut() {
            *item = reader.read_data(header)?;
        }
        Ok(data)
    }
}

impl<T: DataFormat> DataFormat for Vec<T> {
    type Header = T::Header;
    const LATEST_HEADER: Self::Header = T::LATEST_HEADER;

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        let mut written = PackedUint::from(self.len()).write_data(writer)?;
        for item in self.iter() {
            written += writer.write_data(item)?;
        }
        Ok(written)
    }

    fn read_data<R: Read>(reader: &mut R, header: &Self::Header) -> Result<Self, DataReadError> {
        let len = usize::from(PackedUint::read_data(reader, &())?);
        let mut data = Vec::with_capacity(len);
        for _ in 0..len {
            data.push(reader.read_data(header)?);
        }
        Ok(data)
    }
}

impl<T> DataFormat for HashSet<T>
where
    T: DataFormat + Eq + std::hash::Hash,
{
    type Header = T::Header;
    const LATEST_HEADER: Self::Header = T::LATEST_HEADER;

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        let mut written = PackedUint::from(self.len()).write_data(writer)?;
        for item in self.iter() {
            written += writer.write_data(item)?;
        }
        Ok(written)
    }

    fn read_data<R: Read>(reader: &mut R, header: &Self::Header) -> Result<Self, DataReadError> {
        let len = usize::from(PackedUint::read_data(reader, &())?);
        let mut data = HashSet::with_capacity(len);
        for _ in 0..len {
            data.insert(reader.read_data(header)?);
        }
        Ok(data)
    }
}

impl<K, V> DataFormat for HashMap<K, V>
where
    K: DataFormat + Eq + std::hash::Hash,
    V: DataFormat,
{
    type Header = (K::Header, V::Header);
    const LATEST_HEADER: Self::Header = (K::LATEST_HEADER, V::LATEST_HEADER);

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        let mut written = PackedUint::from(self.len()).write_data(writer)?;
        for (key, value) in self.iter() {
            written += writer.write_data(key)?;
            written += writer.write_data(value)?;
        }
        Ok(written)
    }

    fn read_data<R: Read>(
        reader: &mut R,
        (key_header, value_header): &Self::Header,
    ) -> Result<Self, DataReadError> {
        let len = usize::from(PackedUint::read_data(reader, &())?);
        let mut data = HashMap::with_capacity(len);
        for _ in 0..len {
            data.insert(
                reader.read_data(key_header)?,
                reader.read_data(value_header)?,
            );
        }
        Ok(data)
    }
}
