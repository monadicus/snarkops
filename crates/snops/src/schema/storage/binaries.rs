use std::{
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::PathBuf,
    str::FromStr,
};

use lazy_static::lazy_static;
use lazysort::SortedBy;
use serde::{Deserialize, Serialize};
use snops_common::{
    binaries::{BinaryEntry, BinarySource},
    util::sha256_file,
};
use thiserror::Error;

const PROFILES: [&str; 4] = ["release-small", "release", "release-big", "debug"];

lazy_static! {
    pub static ref DEFAULT_AOT_BINARY: BinaryEntry = env_or_bin("snarkos-aot", "AOT_BIN");
    pub static ref DEFAULT_AGENT_BINARY: BinaryEntry = env_or_bin("snops-agent", "AGENT_BIN");
}

/// Get the path to the snarkos-aot binary
fn env_or_bin(name: &str, env: &str) -> BinaryEntry {
    let source = if let Ok(var) = std::env::var(env) {
        BinarySource::from_str(&var)
            .unwrap_or_else(|e| panic!("{env}: failed to parse `{var}` as a binary source: {e:#?}"))
    } else {
        let path = find_bin(name)
            .unwrap_or_else(|| panic!("failed to find binary `{name}`\nSet your {env} environment variable to the path of the binary, or compile the snarkos-aot binary"));

        check_bin(&path).unwrap_or_else(|e| {
            panic!(
                "{env}: failed to resolve binary source `{}`: {e:#?}",
                path.display()
            )
        });
        BinarySource::Path(path)
    };

    let mut entry = BinaryEntry {
        size: None,
        sha256: None,
        source: source.clone(),
    };

    match source {
        BinarySource::Url(_) => {
            if let Ok(size) = std::env::var(format!("{}_SIZE", env)) {
                entry.size = Some(size.parse().unwrap_or_else(|e| {
                    panic!(
                        "{env}_SIZE: failed to parse `{size}` as a u64: {e}",
                        size = size
                    )
                }));
            }
            if let Ok(sha256) = std::env::var(format!("{}_SHA256", env)) {
                entry.sha256 = Some(sha256.to_lowercase());
                if !entry.check_sha256() {
                    panic!("{env}_SHA256: invalid sha256 `{sha256}`");
                }
            }
        }
        BinarySource::Path(path) => {
            entry.sha256 = Some(sha256_file(&path).unwrap_or_else(|e| {
                panic!("failed to calculate sha256 of `{}`: {e}", path.display())
            }));
            entry.size = Some(
                path.metadata()
                    .unwrap_or_else(|e| {
                        panic!("failed to get file metadata of `{}`: {e}", path.display())
                    })
                    .size(),
            );
        }
    }

    entry
}

/// Given the name of a binary file, pick the most recently updated binary
/// out of all the profiles
fn find_bin(name: &str) -> Option<PathBuf> {
    // search in the compilation directory
    let target_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/");

    // walk through the profiles and find the most recently updated binary
    // out of all the profiles, ignoring
    let (path, _created) = PROFILES
        .iter()
        .filter_map(|profile| {
            let file = target_dir.join(format!("{profile}/{name}"));
            if !file.exists() {
                return None;
            }

            file.metadata()
                .and_then(|p| p.created())
                .ok()
                .map(|create_time| (file, create_time))
        })
        .sorted_by(|(_, a_created), (_, b_created)| b_created.cmp(a_created))
        .next()?;

    Some(path)
}

/// Resolve a binary source into a path, downloading the binary if necessary
fn check_bin(path: &PathBuf) -> Result<(), BinResolveError> {
    // ensure target path exists
    if !path.exists() {
        return Err(BinResolveError::NonExistant(path.clone()));
    }

    // ensure file permissions are set execute
    let perms = path
        .metadata()
        .map_err(|e| BinResolveError::AccessDenied(path.clone(), e))?
        .permissions();
    if perms.mode() != 0o755 {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| BinResolveError::SetPermissions(path.clone(), e))?;
    }

    Ok(())
}

#[derive(Debug, Error)]
pub enum BinResolveError {
    #[error("failed to parse binary source: {0}")]
    UrlParse(#[from] url::ParseError),
    #[error("binary does not exist at path: {0}")]
    NonExistant(PathBuf),
    #[error("failed to access binary at path: {0}")]
    AccessDenied(PathBuf, #[source] std::io::Error),
    #[error("failed to set permissions on binary at path: {0}")]
    SetPermissions(PathBuf, #[source] std::io::Error),
}

/// A BinaryEntryDoc can be a shorthand or a full entry
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum BinaryEntryDoc {
    Shorthand(BinarySource),
    Full(BinaryEntry),
}

impl From<BinaryEntryDoc> for BinaryEntry {
    fn from(doc: BinaryEntryDoc) -> Self {
        match doc {
            BinaryEntryDoc::Shorthand(source) => BinaryEntry {
                source,
                sha256: None,
                size: None,
            },
            BinaryEntryDoc::Full(entry) => entry,
        }
    }
}
