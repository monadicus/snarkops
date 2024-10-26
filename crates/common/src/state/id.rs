use std::str::FromStr;

use rand::RngCore;
use serde::de::Error;

use super::INTERNED_ID_REGEX;
use crate::{format::DataFormat, INTERN};

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
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

    pub fn compute_id() -> Self {
        Self(INTERN.get_or_intern("compute"))
    }
}

impl Default for InternedId {
    fn default() -> Self {
        Self(INTERN.get_or_intern("default"))
    }
}

impl std::cmp::PartialOrd for InternedId {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(std::convert::AsRef::<str>::as_ref(self).cmp(other.as_ref()))
    }
}

impl Ord for InternedId {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        std::convert::AsRef::<str>::as_ref(self).cmp(other.as_ref())
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

impl DataFormat for InternedId {
    type Header = ();
    const LATEST_HEADER: Self::Header = ();

    fn write_data<W: std::io::prelude::Write>(
        &self,
        writer: &mut W,
    ) -> Result<usize, crate::format::DataWriteError> {
        self.0.write_data(writer)
    }

    fn read_data<R: std::io::prelude::Read>(
        reader: &mut R,
        _header: &Self::Header,
    ) -> Result<Self, crate::format::DataReadError> {
        Ok(InternedId(lasso::Spur::read_data(reader, &())?))
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_interned_id() {
        let id = InternedId::rand();
        let s = id.to_string();
        let id2 = InternedId::from_str(&s).unwrap();
        assert_eq!(id, id2);
    }

    #[test]
    fn test_interned_id_dataformat() {
        let id = InternedId::rand();
        let mut buf = Vec::new();
        id.write_data(&mut buf).unwrap();
        let id2 = InternedId::read_data(&mut buf.as_slice(), &()).unwrap();
        assert_eq!(id, id2);
    }
}
