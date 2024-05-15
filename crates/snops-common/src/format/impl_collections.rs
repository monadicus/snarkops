use std::{
    collections::{HashMap, HashSet},
    io::{Read, Write},
};

use indexmap::IndexMap;

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

impl<K, V> DataFormat for IndexMap<K, V>
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
        let mut data = IndexMap::with_capacity(len);
        for _ in 0..len {
            data.insert(
                reader.read_data(key_header)?,
                reader.read_data(value_header)?,
            );
        }
        Ok(data)
    }
}

#[cfg(test)]
#[rustfmt::skip]
mod test {
    use crate::format::DataFormat;

    macro_rules! case {
        ($name:ident, $ty:ty, $a:expr, $b:expr) => {
            #[test]
            fn $name() {
                let mut data = Vec::new();
                let value: $ty = $a;
                value.write_data(&mut data).unwrap();
                // we're not doing an assert here because
                // the order of the elements in the collection is not guaranteed
                // assert_eq!(data, &$b);

                let mut reader = &data[..];
                let read_value = <$ty>::read_data(&mut reader, &<$ty as DataFormat>::LATEST_HEADER).unwrap();
                assert_eq!(read_value, value);

            }

        };
    }

    case!(test_vec_u8, Vec<u8>, vec![1, 2, 3], [
        1, 3,
        1, 2, 3
    ]);
    case!(test_vec_u16, Vec<u16>, vec![1, 2, 3], [
        1, 3,
        1, 0,
        2, 0,
        3, 0
    ]);

    case!(test_hashset_u8, std::collections::HashSet<u8>, [1, 2, 3].into_iter().collect(), [
        1, 3,
        1, 2, 3
    ]);
    case!(test_hashset_u16, std::collections::HashSet<u16>, [1, 2, 3].into_iter().collect(), [
        1, 3,
        1, 0,
        2, 0,
        3, 0
    ]);

    case!(test_hashmap_u8_u16, std::collections::HashMap<u8, u16>, [(1, 2), (3, 4)].into_iter().collect(), [
        1, 4,
        1, 0,
        2, 0,
        3, 0,
        4, 0
    ]);
}
