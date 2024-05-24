use std::{fmt, time::Duration};

use indexmap::IndexMap;
use serde::{
    de::{Error, Visitor},
    Deserialize, Deserializer, Serialize,
};
use snops_common::{
    aot_cmds::AotCmd,
    state::{CannonId, DocHeightRequest, InternedId, NodeKey},
};

use super::NodeTargets;
use crate::{
    cannon::{error::AuthorizeError, Authorization},
    env::{error::ExecutionError, Environment},
};

/// A document describing a test's event timeline.
#[derive(Deserialize, Debug, Clone)]
pub struct Document {
    pub name: InternedId,
    pub description: Option<String>,
    pub timeline: Vec<TimelineEvent>,
}

/// An event in the test timeline.
#[derive(Deserialize, Debug, Clone)]
pub struct TimelineEvent {
    /// The event will run for at least the given duration
    pub duration: Option<EventDuration>,

    /// An awaited action will error if it does not occur within the given
    /// duration
    pub timeout: Option<EventDuration>,

    #[serde(flatten)]
    pub actions: Actions,
}

#[derive(Debug, Clone)]
pub struct Actions(pub Vec<ActionInstance>);

#[derive(Debug, Clone)]
pub struct ActionInstance {
    pub action: Action,
    pub awaited: bool,
}

#[derive(Debug, Clone)]
pub enum Action {
    /// Update the given nodes to an online state
    Online(NodeTargets),
    /// Update the given nodes to an offline state
    Offline(NodeTargets),
    /// Fire transactions from a source file at a target node
    Cannon(Vec<SpawnCannon>),
    /// Set the height of some nodes' ledgers
    Config(IndexMap<NodeTargets, Reconfig>),
    /// Execute
    Execute(Execute),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum Execute {
    /// Execute a program
    #[serde(rename_all = "kebab-case")]
    Program {
        private_key: String,
        /// The program to execute
        program: String,
        /// The function to call
        function: String,
        /// The cannon id of who to execute the transaction
        cannon: CannonId,
        /// The inputs to the function
        inputs: Vec<String>,
        /// The optional priority fee
        #[serde(default)]
        priority_fee: Option<u64>,
        /// The optional fee record for a private fee
        #[serde(default)]
        fee_record: Option<String>,
    },
    Transaction {
        /// The transaction to execute
        tx: String,
        /// The cannon id of who to execute the transaction
        cannon: CannonId,
    },
}

impl Execute {
    pub async fn execute(&self, env: &Environment) -> Result<(), ExecutionError> {
        match self {
            Execute::Program {
                cannon: cannon_id,
                private_key,
                program,
                function,
                // TODO: parse the inputs as values like `key/committee.0`
                inputs,
                priority_fee,
                fee_record,
            } => {
                let Some(cannon) = env.cannons.get(cannon_id) else {
                    return Err(ExecutionError::UnknownCannon(*cannon_id));
                };

                // authorize the transaction
                let auth = AotCmd::new(env.aot_bin.clone(), env.network)
                    .authorize_program(private_key, program, function, inputs)
                    .await?;

                // authorize the transaction
                let fee_auth = AotCmd::new(env.aot_bin.clone(), env.network)
                    .authorize_fee(private_key, &auth, *priority_fee, fee_record.as_ref())
                    .await?;

                // parse the json and bundle it up
                let authorization = Authorization {
                    auth: serde_json::from_str(&auth).map_err(AuthorizeError::Json)?,
                    fee_auth: Some(serde_json::from_str(&fee_auth).map_err(AuthorizeError::Json)?),
                };

                // proxy it to a listen cannon
                cannon.proxy_auth(authorization)?;
                Ok(())
            }
            Execute::Transaction { .. } => {
                todo!("locate the transaction id from some kind of database, then broadcast it to the cannon")
            }
        }
    }
}

impl<'de> Deserialize<'de> for Actions {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ActionsVisitor;

        impl<'de> Visitor<'de> for ActionsVisitor {
            type Value = Actions;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("possibly awaited action map")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let mut buf = vec![];

                while let Some(key) = map.next_key::<&str>()? {
                    // determine if this action is being awaited
                    let (key, awaited) = match key {
                        key if key.ends_with(".await") => (key.split_at(key.len() - 6).0, true),
                        _ => (key, false),
                    };

                    buf.push(ActionInstance {
                        awaited,
                        action: match key {
                            "online" => Action::Online(map.next_value()?),
                            "offline" => Action::Offline(map.next_value()?),
                            "cannon" => Action::Cannon(map.next_value()?),
                            "config" => Action::Config(map.next_value()?),
                            "execute" => Action::Execute(map.next_value()?),

                            _ => return Err(A::Error::custom(format!("unsupported action {key}"))),
                        },
                    });
                }

                Ok(Actions(buf))
            }
        }

        deserializer.deserialize_map(ActionsVisitor)
    }
}

#[derive(Debug, Clone)]
pub enum EventDuration {
    Time(Duration),
    Blocks(u64),
}

impl<'de> Deserialize<'de> for EventDuration {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct EventDurationVisitor;

        impl<'de> Visitor<'de> for EventDurationVisitor {
            type Value = EventDuration;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter
                    .write_str("a string duration or an integer number of blocks to be produced")
            }

            fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(EventDuration::Blocks(v))
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(EventDuration::Time(
                    duration_str::parse(v).map_err(E::custom)?,
                ))
            }
        }

        deserializer.deserialize_any(EventDurationVisitor)
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct SpawnCannon {
    pub name: CannonId,
    #[serde(default)]
    pub count: Option<usize>,
    /// overwrite the query's source node
    #[serde(default)]
    pub query: Option<NodeKey>,
    /// overwrite the cannon sink target
    #[serde(default)]
    pub target: Option<NodeTargets>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Reconfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<DocHeightRequest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peers: Option<NodeTargets>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validators: Option<NodeTargets>,
}
