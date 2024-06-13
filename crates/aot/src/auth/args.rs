use std::{path::PathBuf, str::FromStr};

use anyhow::anyhow;
use clap::{Args, Parser};
use clap_stdin::MaybeStdin;
use serde::{Deserialize, Serialize};
use snarkvm::{
    ledger::Deployment,
    prelude::{PrivateKey, ProgramOwner},
    synthesizer::Authorization,
    utilities::DeserializeExt,
};

use crate::{runner::Key, Network};

#[derive(Clone, Debug, Parser)]
pub struct AuthArgs<N: Network> {
    /// Authorization of the program function
    #[clap(short, long)]
    pub auth: Option<Authorization<N>>,
    #[clap(short, long)]
    pub fee_auth: Option<Authorization<N>>,
    #[clap(short, long)]
    pub owner: Option<ProgramOwner<N>>,
    #[clap(short, long)]
    pub deployment: Option<Deployment<N>>,
    /// Authorization flags as json
    ///
    /// `{"auth": Program Auth, "fee_auth": Fee Auth }`
    ///
    /// `{"deployment": Deployment, "owner": Prog Owner, "fee_auth": Fee Auth }`
    json: Option<MaybeStdin<AuthBlob<N>>>,
}

impl<N: Network> AuthArgs<N> {
    pub fn pick(self) -> anyhow::Result<AuthBlob<N>> {
        self.json
            .map(MaybeStdin::into_inner)
            .or_else(|| match (self.auth, self.owner, self.deployment) {
                (Some(auth), None, None) => Some(AuthBlob::Program {
                    auth,
                    fee_auth: self.fee_auth,
                }),
                (None, Some(owner), Some(deployment)) => Some(AuthBlob::Deploy {
                    owner,
                    deployment,
                    fee_auth: self.fee_auth,
                }),
                _ => None,
            })
            .ok_or(anyhow!("No authorization provided"))
    }
}

/*
    | json: { auth, fee_auth }
    | json: { deployment, owner, fee_auth }

    | --auth --fee_auth
    | --deployment --owner --fee_auth
*/

#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub enum AuthBlob<N: Network> {
    Program {
        auth: Authorization<N>,
        fee_auth: Option<Authorization<N>>,
    },
    Deploy {
        owner: ProgramOwner<N>,
        deployment: Deployment<N>,
        #[serde(skip_serializing_if = "Option::is_none")]
        fee_auth: Option<Authorization<N>>,
    },
}

impl<'de, N: Network> Deserialize<'de> for AuthBlob<N> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let mut value = serde_json::Value::deserialize(deserializer)?;
        let fee_auth = DeserializeExt::take_from_value::<D>(&mut value, "fee_auth")?;

        if value.get("auth").is_some() {
            Ok(Self::Program {
                fee_auth,
                auth: DeserializeExt::take_from_value::<D>(&mut value, "auth")?,
            })
        } else {
            Ok(Self::Deploy {
                fee_auth,
                owner: DeserializeExt::take_from_value::<D>(&mut value, "owner")?,
                deployment: DeserializeExt::take_from_value::<D>(&mut value, "deployment")?,
            })
        }
    }
}

impl<N: Network> FromStr for AuthBlob<N> {
    type Err = serde_json::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        serde_json::from_str(s)
    }
}

#[derive(Debug, Args, Clone)]
#[group(multiple = false)]
pub struct FeeKey<N: Network> {
    /// Specify the account private key of the node
    #[clap(long = "fee-private-key")]
    pub fee_private_key: Option<PrivateKey<N>>,
    /// Specify the account private key of the node
    #[clap(long = "fee-private-key-file")]
    pub fee_private_key_file: Option<PathBuf>,
}

impl<N: Network> FeeKey<N> {
    pub fn get(self) -> Option<PrivateKey<N>> {
        match (self.fee_private_key, self.fee_private_key_file) {
            (Some(key), None) => Some(key),
            (None, Some(file)) => {
                let raw = std::fs::read_to_string(file).ok()?.trim().to_string();
                PrivateKey::from_str(&raw).ok()
            }
            _ => None,
        }
    }

    pub fn as_key(self) -> Option<Key<N>> {
        Some(Key {
            // this might seem redundant, but it `None` instead of `Some({ private_key: None, ...
            // })`
            private_key: Some(self.get()?),
            private_key_file: None,
        })
    }
}
