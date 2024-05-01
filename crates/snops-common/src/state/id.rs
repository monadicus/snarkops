use std::str::FromStr;

use rand::RngCore;
use serde::de::Error;

use super::INTERNED_ID_REGEX;
use crate::INTERN;

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InternedId(lasso::Spur);

impl InternedId {
    pub fn rand() -> Self {
        let id = rand::thread_rng().next_u32();
        Self(INTERN.get_or_intern(format!("unknown-{}", id)))
    }

    pub fn into_inner(self) -> u32 {
        self.0.into_inner().get()
    }

    pub fn is_match(s: &str) -> bool {
        INTERNED_ID_REGEX.is_match(s)
    }
}

/// To prevent the risk of memory leaking agent/env ids that are not used, we
/// check if the id is interned before from-stringing it
pub fn id_or_none<T: FromStr>(s: &str) -> Option<T> {
    if !INTERN.contains(s) {
        return None;
    }
    T::from_str(s).ok()
}

impl FromStr for InternedId {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if !InternedId::is_match(s) {
            return Err(format!(
                "invalid {} expected pattern [A-Za-z0-9][A-Za-z0-9\\-_.]{{,63}}",
                stringify!(InternedId)
            ));
        }

        Ok(InternedId(INTERN.get_or_intern(s)))
    }
}

impl<'de> serde::Deserialize<'de> for InternedId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::from_str(&s).map_err(D::Error::custom)
    }
}

impl std::fmt::Display for InternedId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", INTERN.resolve(&self.0))
    }
}

impl AsRef<str> for InternedId {
    fn as_ref(&self) -> &str {
        INTERN.resolve(&self.0)
    }
}

impl AsRef<[u8]> for InternedId {
    fn as_ref(&self) -> &[u8] {
        INTERN.resolve(&self.0).as_bytes()
    }
}

impl serde::Serialize for InternedId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_ref())
    }
}
