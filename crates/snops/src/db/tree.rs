use bytes::Buf;
use snops_common::format::{read_dataformat, DataFormat};

use super::error::DatabaseError;

pub struct DbTree<Key: DataFormat, Value: DataFormat> {
    tree: sled::Tree,
    _phantom: std::marker::PhantomData<(Key, Value)>,
}

impl<Key: DataFormat<Header = ()>, Value: DataFormat> DbTree<Key, Value> {
    pub fn new(tree: sled::Tree) -> Self {
        Self {
            tree,
            _phantom: std::marker::PhantomData,
        }
    }

    pub fn read_all(&self) -> impl Iterator<Item = (Key, Value)> {
        self.tree.iter().filter_map(|row| {
            let (key_bytes, value_bytes) = match row {
                Ok((key, value)) => (key, value),
                Err(e) => {
                    tracing::error!("Error reading row from store: {e}");
                    return None;
                }
            };

            let key = match Key::read_data(&mut key_bytes.reader(), &()) {
                Ok(key) => key,
                Err(e) => {
                    tracing::error!("Error parsing key from store: {e}");
                    return None;
                }
            };

            let value = match read_dataformat(&mut value_bytes.reader()) {
                Ok(value) => value,
                Err(e) => {
                    tracing::error!("Error parsing value from store: {e}");
                    return None;
                }
            };

            Some((key, value))
        })
    }

    pub fn save(&self, key: Key, value: Value) -> Result<(), DatabaseError> {
        self.tree.insert(key.to_byte_vec()?, value.to_byte_vec()?)?;
        Ok(())
    }

    pub fn delete(&self, key: Key) -> Result<bool, DatabaseError> {
        Ok(self.tree.remove(key.to_byte_vec()?)?.is_some())
    }
}
