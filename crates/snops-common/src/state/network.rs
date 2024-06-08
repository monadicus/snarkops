use serde::{Deserialize, Serialize};

use crate::format::DataFormat;

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum NetworkId {
    #[default]
    Mainnet,
    Testnet,
    Canary,
}

impl std::str::FromStr for NetworkId {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "mainnet" => Ok(Self::Mainnet),
            "testnet" => Ok(Self::Testnet),
            "canary" => Ok(Self::Canary),
            _ => Err("Invalid network ID"),
        }
    }
}

impl std::fmt::Display for NetworkId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Mainnet => write!(f, "mainnet"),
            Self::Testnet => write!(f, "testnet"),
            Self::Canary => write!(f, "canary"),
        }
    }
}

impl DataFormat for NetworkId {
    type Header = u8;
    const LATEST_HEADER: Self::Header = 1u8;

    fn write_data<W: std::io::prelude::Write>(
        &self,
        writer: &mut W,
    ) -> Result<usize, crate::format::DataWriteError> {
        match self {
            Self::Mainnet => 0u8.write_data(writer),
            Self::Testnet => 1u8.write_data(writer),
            Self::Canary => 2u8.write_data(writer),
        }
    }

    fn read_data<R: std::io::prelude::Read>(
        reader: &mut R,
        header: &Self::Header,
    ) -> Result<Self, crate::format::DataReadError> {
        if *header != Self::LATEST_HEADER {
            return Err(crate::format::DataReadError::unsupported(
                "network_id",
                Self::LATEST_HEADER,
                *header,
            ));
        }

        match u8::read_data(reader, &())? {
            0 => Ok(Self::Mainnet),
            1 => Ok(Self::Testnet),
            2 => Ok(Self::Canary),
            n => Err(crate::format::DataReadError::Custom(format!(
                "Invalid network ID: {n}"
            ))),
        }
    }
}
