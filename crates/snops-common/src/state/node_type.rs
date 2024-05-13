use std::ffi::OsStr;

use crate::format::DataFormat;

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeType {
    Client = 0,
    Validator = 1,
    Prover = 2,
}

impl AsRef<str> for NodeType {
    fn as_ref(&self) -> &str {
        match self {
            Self::Client => "client",
            Self::Validator => "validator",
            Self::Prover => "prover",
        }
    }
}

impl AsRef<OsStr> for NodeType {
    fn as_ref(&self) -> &OsStr {
        OsStr::new(self)
    }
}

impl NodeType {
    pub fn flag(self) -> &'static str {
        match self {
            Self::Client => "--client",
            Self::Validator => "--validator",
            Self::Prover => "--prover",
        }
    }

    pub fn bit(self) -> usize {
        self as usize
    }
}

impl std::fmt::Display for NodeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
}

impl std::str::FromStr for NodeType {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "client" => Ok(Self::Client),
            "validator" => Ok(Self::Validator),
            "prover" => Ok(Self::Prover),
            _ => Err("invalid node type string"),
        }
    }
}

impl DataFormat for NodeType {
    type Header = u8;
    const LATEST_HEADER: Self::Header = 1;

    fn write_data<W: std::io::Write>(
        &self,
        writer: &mut W,
    ) -> Result<usize, crate::format::DataWriteError> {
        Ok(writer.write(&[match self {
            Self::Client => 0,
            Self::Validator => 1,
            Self::Prover => 2,
        }])?)
    }

    fn read_data<R: std::io::Read>(
        reader: &mut R,
        header: &Self::Header,
    ) -> Result<Self, crate::format::DataReadError> {
        if *header != Self::LATEST_HEADER {
            return Err(crate::format::DataReadError::unsupported(
                "NodeType",
                Self::LATEST_HEADER,
                *header,
            ));
        }

        let mut byte = [0u8; 1];
        reader.read_exact(&mut byte)?;
        match byte[0] {
            0 => Ok(Self::Client),
            1 => Ok(Self::Validator),
            2 => Ok(Self::Prover),
            n => Err(crate::format::DataReadError::Custom(format!(
                "invalid NodeType tag {n}, expected 0, 1, or 2"
            ))),
        }
    }
}
