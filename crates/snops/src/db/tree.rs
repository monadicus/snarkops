use bytes::Buf;
use snops_common::format::{read_dataformat, write_dataformat, DataFormat};

use super::error::DatabaseError;

pub struct DbTree<Key: DataFormat, Value: DataFormat> {
    tree: sled::Tree,
    _phantom: std::marker::PhantomData<(Key, Value)>,
}

impl<Key: DataFormat, Value: DataFormat> DbTree<Key, Value> {
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

            let key = match Key::read_data(&mut key_bytes.reader(), &Key::LATEST_HEADER) {
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

    pub fn read_with_prefix<Prefix: DataFormat>(
        &self,
        prefix: &Prefix,
    ) -> Result<impl Iterator<Item = (Key, Value)>, DatabaseError> {
        Ok(self
            .tree
            .scan_prefix(prefix.to_byte_vec()?)
            .filter_map(|row| {
                let (key_bytes, value_bytes) = match row {
                    Ok((key, value)) => (key, value),
                    Err(e) => {
                        tracing::error!("Error reading row from store: {e}");
                        return None;
                    }
                };

                let key = match Key::read_data(&mut key_bytes.reader(), &Key::LATEST_HEADER) {
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
            }))
    }

    pub fn restore(&self, key: &Key) -> Result<Option<Value>, DatabaseError> {
        Ok(self
            .tree
            .get(key.to_byte_vec()?)?
            .map(|value_bytes| read_dataformat(&mut value_bytes.reader()))
            .transpose()?)
    }

    pub fn save(&self, key: &Key, value: &Value) -> Result<(), DatabaseError> {
        let key_bytes = key.to_byte_vec()?;
        let mut value_bytes = Vec::new();
        write_dataformat(&mut value_bytes, value)?;
        self.tree.insert(key_bytes, value_bytes)?;
        Ok(())
    }

    pub fn delete(&self, key: &Key) -> Result<bool, DatabaseError> {
        Ok(self.tree.remove(key.to_byte_vec()?)?.is_some())
    }

    pub fn delete_with_prefix<Prefix: DataFormat>(
        &self,
        prefix: &Prefix,
    ) -> Result<usize, DatabaseError> {
        Ok(self
            .tree
            .scan_prefix(prefix.to_byte_vec()?)
            .map(|row| {
                let key_bytes = match row {
                    Ok((key, _)) => key,
                    Err(e) => {
                        tracing::error!("Error reading row from store: {e}");
                        return 0;
                    }
                };

                let key = match Key::read_data(&mut key_bytes.reader(), &Key::LATEST_HEADER) {
                    Ok(key) => key,
                    Err(e) => {
                        tracing::error!("Error parsing key from store: {e}");
                        return 0;
                    }
                };

                if let Err(e) = self.delete(&key) {
                    tracing::error!("Error deleting key from store: {e}");
                    return 0;
                }

                1
            })
            .sum())
    }
}
