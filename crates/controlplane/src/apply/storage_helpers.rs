use std::path::PathBuf;

use indexmap::IndexMap;
use serde::de::DeserializeOwned;

use super::{error::StorageError, AleoAddrMap};

// TODO: function should also take storage id
// in case of error, the storage id can be used to provide more context
pub async fn read_to_addrs<T: DeserializeOwned>(
    f: impl Fn(T) -> String,
    file: &PathBuf,
) -> Result<AleoAddrMap, StorageError> {
    if !file.exists() {
        return Ok(Default::default());
    }

    let data = tokio::fs::read_to_string(file)
        .await
        .map_err(|e| StorageError::ReadBalances(file.clone(), e))?;
    let parsed: IndexMap<String, T> =
        serde_json::from_str(&data).map_err(|e| StorageError::ParseBalances(file.clone(), e))?;

    Ok(parsed.into_iter().map(|(k, v)| (k, f(v))).collect())
}

pub async fn get_version_from_path(path: &PathBuf) -> Result<Option<u16>, StorageError> {
    if !path.exists() {
        return Ok(None);
    }

    let data = tokio::fs::read_to_string(path)
        .await
        .map_err(|e| StorageError::ReadVersion(path.clone(), e))?;

    Ok(data.parse().ok())
}

pub fn pick_additional_addr(entry: (String, u64, Option<serde_json::Value>)) -> String {
    entry.0
}
pub fn pick_commitee_addr(entry: (String, u64)) -> String {
    entry.0
}
pub fn pick_account_addr(entry: String) -> String {
    entry
}
