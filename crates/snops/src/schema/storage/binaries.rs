use serde::{Deserialize, Serialize};
use snops_common::binaries::{BinaryEntry, BinarySource};

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

#[cfg(test)]
mod test {
    // test if a random string can parse into a uri:
    #[test]
    fn test_uri() {
        let uri = "http://example.com";
        let parsed = url::Url::parse(uri);
        assert!(parsed.is_ok());
        let uri = "meow/bar/baz";
        let parsed = url::Url::parse(uri);
        assert!(parsed.is_ok());
    }
}
