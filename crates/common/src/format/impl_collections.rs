use std::{
    collections::{HashMap, HashSet},
    io::{Read, Write},
};

use indexmap::{IndexMap, IndexSet};

use super::{
    packed_int::PackedUint, DataFormat, DataFormatReader, DataFormatWriter, DataReadError,
    DataWriteError,
};

/// BytesFormat is a simple wrapper around a Vec<u8> that implements DataFormat
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct BytesFormat(pub Vec<u8>);
impl From<Vec<u8>> for BytesFormat {
    fn from(data: Vec<u8>) -> Self {
        Self(data)
    }
}
impl From<BytesFormat> for Vec<u8> {
    fn from(data: BytesFormat) -> Self {
        data.0
    }
}

impl DataFormat for BytesFormat {
    type Header = ();
    const LATEST_HEADER: Self::Header = ();

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        Ok(PackedUint::from(self.0.len()).write_data(writer)? + writer.write(&self.0)?)
    }

    fn read_data<R: Read>(reader: &mut R, _header: &Self::Header) -> Result<Self, DataReadError> {
        let mut data = vec![0; usize::from(PackedUint::read_data(reader, &())?)];
        reader.read_exact(&mut data)?;
        Ok(Self(data))
    }
}

/// EncodedFormat is a simple wrapper around a DataFormat to encode header data
/// with the data
#[derive(Debug, Clone)]
pub struct EncodedFormat<F: DataFormat>(pub F);

impl<F: DataFormat + PartialEq> PartialEq for EncodedFormat<F> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<F: DataFormat + Eq> Eq for EncodedFormat<F> {}

impl<F: DataFormat> DataFormat for EncodedFormat<F> {
    type Header = ();
    const LATEST_HEADER: Self::Header = ();

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        Ok(self.write_header(writer)? + self.write_data(writer)?)
    }

    fn read_data<R: Read>(reader: &mut R, _header: &Self::Header) -> Result<Self, DataReadError> {
        let header = F::read_header(reader)?;
        Ok(Self(F::read_data(reader, &header)?))
    }
}

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

macro_rules! impl_set {
    ($set_ty:ident) => {
        impl<T> DataFormat for $set_ty<T>
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

            fn read_data<R: Read>(
                reader: &mut R,
                header: &Self::Header,
            ) -> Result<Self, DataReadError> {
                let len = usize::from(PackedUint::read_data(reader, &())?);
                let mut data = $set_ty::with_capacity(len);
                for _ in 0..len {
                    data.insert(reader.read_data(header)?);
                }
                Ok(data)
            }
        }
    };
}

impl_set!(HashSet);
impl_set!(IndexSet);

macro_rules! impl_map {
    ($map_ty:ident) => {
        impl<K, V> DataFormat for $map_ty<K, V>
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
                let mut data = $map_ty::with_capacity(len);
                for _ in 0..len {
                    data.insert(
                        reader.read_data(key_header)?,
                        reader.read_data(value_header)?,
                    );
                }
                Ok(data)
            }
        }
    };
}

impl_map!(HashMap);
impl_map!(IndexMap);

#[cfg(test)]
#[rustfmt::skip]
mod test {
    use crate::format::{BytesFormat, DataFormat};

    macro_rules! case {
        ($name:ident, $ty:ty, $a:expr_2021, $b:expr_2021) => {
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

    // binary data test
    case!(test_binary_data, BytesFormat, BytesFormat(vec![1, 2, 3]), [
        1, 3,
        1,
        2,
        3
    ]);
}
