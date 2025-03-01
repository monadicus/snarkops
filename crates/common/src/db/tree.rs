use bytes::Buf;

use super::error::DatabaseError;
use crate::format::{DataFormat, read_dataformat, write_dataformat};

pub struct DbTree<K, V> {
    tree: sled::Tree,
    _phantom: std::marker::PhantomData<(K, V)>,
}

impl<K: DataFormat, V: DataFormat> DbTree<K, V> {
    pub fn new(tree: sled::Tree) -> Self {
        Self {
            tree,
            _phantom: std::marker::PhantomData,
        }
    }

    pub fn read_all(&self) -> impl Iterator<Item = (K, V)> + use<K, V> {
        self.tree.iter().filter_map(|row| {
            let (key_bytes, value_bytes) = match row {
                Ok((key, value)) => (key, value),
                Err(e) => {
                    tracing::error!("Error reading row from store: {e}");
                    return None;
                }
            };

            let key = match K::read_data(&mut key_bytes.reader(), &K::LATEST_HEADER) {
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
    ) -> Result<impl Iterator<Item = (K, V)> + use<Prefix, K, V>, DatabaseError> {
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

                let key = match K::read_data(&mut key_bytes.reader(), &K::LATEST_HEADER) {
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

    pub fn restore(&self, key: &K) -> Result<Option<V>, DatabaseError> {
        Ok(self
            .tree
            .get(key.to_byte_vec()?)?
            .map(|value_bytes| read_dataformat(&mut value_bytes.reader()))
            .transpose()?)
    }

    pub fn save(&self, key: &K, value: &V) -> Result<(), DatabaseError> {
        let key_bytes = key.to_byte_vec()?;
        let mut value_bytes = Vec::new();
        write_dataformat(&mut value_bytes, value)?;
        self.tree.insert(key_bytes, value_bytes)?;
        Ok(())
    }

    pub fn save_option(&self, key: &K, value: Option<&V>) -> Result<(), DatabaseError> {
        match value {
            Some(value) => self.save(key, value),
            None => self.delete(key).map(|_| ()),
        }
    }

    pub fn delete(&self, key: &K) -> Result<bool, DatabaseError> {
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

                let key = match K::read_data(&mut key_bytes.reader(), &K::LATEST_HEADER) {
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

pub struct DbRecords<K> {
    tree: sled::Tree,
    _phantom: std::marker::PhantomData<K>,
}

impl<K: DataFormat> DbRecords<K> {
    pub fn new(tree: sled::Tree) -> Self {
        Self {
            tree,
            _phantom: std::marker::PhantomData,
        }
    }

    pub fn restore<V: DataFormat>(&self, key: &K) -> Result<Option<V>, DatabaseError> {
        Ok(self
            .tree
            .get(key.to_byte_vec()?)?
            .map(|value_bytes| read_dataformat(&mut value_bytes.reader()))
            .transpose()?)
    }

    pub fn save<V: DataFormat>(&self, key: &K, value: &V) -> Result<(), DatabaseError> {
        let key_bytes = key.to_byte_vec()?;
        let mut value_bytes = Vec::new();
        write_dataformat(&mut value_bytes, value)?;
        self.tree.insert(key_bytes, value_bytes)?;
        Ok(())
    }

    pub fn save_option<V: DataFormat>(
        &self,
        key: &K,
        value: Option<&V>,
    ) -> Result<(), DatabaseError> {
        match value {
            Some(value) => self.save(key, value),
            None => self.delete(key).map(|_| ()),
        }
    }

    pub fn delete(&self, key: &K) -> Result<bool, DatabaseError> {
        Ok(self.tree.remove(key.to_byte_vec()?)?.is_some())
    }
}
