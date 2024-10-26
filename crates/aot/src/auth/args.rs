use std::{path::PathBuf, str::FromStr};

use anyhow::anyhow;
use clap::{Args, Parser};
use clap_stdin::MaybeStdin;
use serde::{Deserialize, Serialize};
use snarkvm::{
    ledger::{Deployment, Transition},
    prelude::{PrivateKey, ProgramOwner, Request},
    synthesizer::Authorization,
    utilities::DeserializeExt,
};

use crate::{Key, Network};

/// The authorization arguments.
#[derive(Clone, Debug, Parser)]
pub struct AuthArgs<N: Network> {
    /// Authorization for an execution of some kind.
    #[clap(short, long)]
    pub auth: Option<ProxyAuthorization<N>>,
    /// The optional fee authorization for said execution.
    #[clap(short, long)]
    pub fee_auth: Option<ProxyAuthorization<N>>,
    /// The owner of the program if deploying.
    #[clap(short, long)]
    pub owner: Option<ProgramOwner<N>>,
    /// The deployment of the program if deploying.
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
                    deployment: Box::new(deployment),
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
        /// The authorization for the program.
        auth: ProxyAuthorization<N>,
        /// The optional fee authorization for the program.
        fee_auth: Option<ProxyAuthorization<N>>,
    },
    Deploy {
        /// The owner of the program.
        owner: ProgramOwner<N>,
        /// The deployment of the program.
        deployment: Box<Deployment<N>>,
        /// The optional fee authorization for the deployment.
        #[serde(skip_serializing_if = "Option::is_none")]
        fee_auth: Option<ProxyAuthorization<N>>,
    },
}

impl<'de, N: Network> Deserialize<'de> for AuthBlob<N> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let mut value = serde_json::Value::deserialize(deserializer)?;
        let fee_auth = value
            .get("fee_auth")
            .is_some()
            .then(|| DeserializeExt::take_from_value::<D>(&mut value, "fee_auth"))
            .transpose()?;

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

/// This type exists because aleo's Authorization::try_from((Vec<Request>,
/// Vec<Transition>)) has a bug that prevents deserialization from working on
/// programs with multiple transitions
///
/// This is a wrapper that converts to and from authorizations
#[derive(Clone, Debug, Serialize)]
pub struct ProxyAuthorization<N: Network> {
    pub requests: Vec<Request<N>>,
    pub transitions: Vec<Transition<N>>,
}

impl<'de, N: Network> Deserialize<'de> for ProxyAuthorization<N> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let mut value = serde_json::Value::deserialize(deserializer)?;
        Ok(Self {
            requests: DeserializeExt::take_from_value::<D>(&mut value, "requests")?,
            transitions: DeserializeExt::take_from_value::<D>(&mut value, "transitions")?,
        })
    }
}

impl<N: Network> FromStr for ProxyAuthorization<N> {
    type Err = serde_json::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        serde_json::from_str(s)
    }
}

impl<N: Network> From<ProxyAuthorization<N>> for Authorization<N> {
    fn from(auth: ProxyAuthorization<N>) -> Self {
        let new_auth = Authorization::try_from((vec![], vec![])).unwrap();
        for req in auth.requests {
            new_auth.push(req);
        }
        for transition in auth.transitions {
            let _ = new_auth.insert_transition(transition);
        }

        new_auth
    }
}

impl<N: Network> From<Authorization<N>> for ProxyAuthorization<N> {
    fn from(auth: Authorization<N>) -> Self {
        let mut requests = vec![];
        let mut transitions = vec![];

        for req in auth.to_vec_deque() {
            requests.push(req.clone());
        }
        for transition in auth.transitions().values() {
            transitions.push(transition.clone());
        }

        Self {
            requests,
            transitions,
        }
    }
}

/// A private key for the fee account.
/// Either a private key or a file containing the private key.
#[derive(Debug, Args, Clone)]
#[group(multiple = false)]
pub struct FeeKey<N: Network> {
    /// Specify the account private key of the node
    #[clap(env, long)]
    pub fee_private_key: Option<PrivateKey<N>>,
    /// Specify the account private key of the node
    #[clap(env, long)]
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
