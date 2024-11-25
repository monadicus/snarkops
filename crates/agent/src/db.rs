use std::{
    io::{Read, Write},
    net::IpAddr,
    path::Path,
    sync::{Arc, Mutex},
};

use bytes::Buf;
use indexmap::IndexMap;
use snops_common::{
    api::EnvInfo,
    db::{error::DatabaseError, tree::DbTree, Database as DatabaseTrait},
    format::{self, read_dataformat, DataFormat, DataReadError, DataWriteError, PackedUint},
    state::{AgentId, AgentState, EnvId, HeightRequest},
};
use url::Url;

use crate::reconcile::agent::EnvState;

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
#[repr(u8)]
pub enum AgentDbString {
    /// JSON web token of agent.
    Jwt,
    /// Process ID of node. Used to keep track of zombie node processes.
    NodePid,
    // Url to Loki instance, configured by the endpoint.
    LokiUrl,
    /// Current state of the agent.
    AgentState,
    /// Current environment state.
    EnvState,
    /// Latest stored environment info.
    EnvInfo,
    /// Agent addresses resolved by the controlplane.
    ResolvedAddrs,
    /// Last height of the agent state
    LastHeight,
}

impl DataFormat for AgentDbString {
    type Header = u8;
    const LATEST_HEADER: Self::Header = 1;

    fn read_data<R: Read>(reader: &mut R, header: &Self::Header) -> Result<Self, DataReadError> {
        if *header != Self::LATEST_HEADER {
            return Err(DataReadError::unsupported(
                "AgentDbString",
                Self::LATEST_HEADER,
                header,
            ));
        }

        Ok(match u8::read_data(reader, &())? {
            0 => Self::Jwt,
            1 => Self::NodePid,
            2 => Self::LokiUrl,
            3 => Self::AgentState,
            4 => Self::EnvInfo,
            _ => return Err(DataReadError::custom("invalid agent DB string type")),
        })
    }

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        (*self as u8).write_data(writer)
    }
}

pub struct Database {
    #[allow(unused)]
    pub db: sled::Db,

    pub jwt_mutex: Mutex<Option<String>>,
    pub strings: DbTree<AgentDbString, String>,
    pub documents: DbTree<AgentDbString, format::BinaryData>,
}

impl DatabaseTrait for Database {
    fn open(path: &Path) -> Result<Self, DatabaseError> {
        let db = sled::open(path)?;
        let strings = DbTree::new(db.open_tree(b"v1/strings")?);
        let documents = DbTree::new(db.open_tree(b"v1/documents")?);
        let jwt_mutex = Mutex::new(strings.restore(&AgentDbString::Jwt)?);

        Ok(Self {
            db,
            jwt_mutex,
            strings,
            documents,
        })
    }
}

impl Database {
    pub fn jwt(&self) -> Option<String> {
        self.jwt_mutex.lock().unwrap().clone()
    }

    pub fn set_jwt(&self, jwt: Option<String>) -> Result<(), DatabaseError> {
        let mut lock = self.jwt_mutex.lock().unwrap();
        self.strings
            .save_option(&AgentDbString::Jwt, jwt.as_ref())?;
        *lock = jwt;
        Ok(())
    }

    pub fn set_loki_url(&self, url: Option<String>) -> Result<(), DatabaseError> {
        self.strings
            .save_option(&AgentDbString::LokiUrl, url.as_ref())
    }

    pub fn loki_url(&self) -> Option<Url> {
        self.strings
            .restore(&AgentDbString::LokiUrl)
            .ok()?
            .and_then(|url| url.parse::<Url>().ok())
    }

    pub fn env_info(&self) -> Result<Option<(EnvId, Arc<EnvInfo>)>, DatabaseError> {
        self.documents
            .restore(&AgentDbString::EnvInfo)?
            .map(|format::BinaryData(bytes)| read_dataformat(&mut bytes.reader()))
            .transpose()
            .map_err(DatabaseError::from)
    }

    pub fn set_env_info(&self, info: Option<(EnvId, Arc<EnvInfo>)>) -> Result<(), DatabaseError> {
        if let Some(info) = info {
            self.documents.save(
                &AgentDbString::EnvInfo,
                &format::BinaryData(info.to_byte_vec()?),
            )
        } else {
            self.documents.delete(&AgentDbString::EnvInfo).map(|_| ())
        }
    }

    pub fn agent_state(&self) -> Result<AgentState, DatabaseError> {
        Ok(
            if let Some(format::BinaryData(bytes)) =
                self.documents.restore(&AgentDbString::AgentState)?
            {
                read_dataformat(&mut bytes.reader())?
            } else {
                AgentState::default()
            },
        )
    }

    pub fn set_agent_state(&self, state: &AgentState) -> Result<(), DatabaseError> {
        self.documents.save(
            &AgentDbString::AgentState,
            &format::BinaryData(state.to_byte_vec()?),
        )
    }

    pub fn resolved_addrs(&self) -> Result<IndexMap<AgentId, IpAddr>, DatabaseError> {
        Ok(
            if let Some(format::BinaryData(bytes)) =
                self.documents.restore(&AgentDbString::ResolvedAddrs)?
            {
                read_dataformat(&mut bytes.reader())?
            } else {
                IndexMap::new()
            },
        )
    }

    pub fn set_resolved_addrs(
        &self,
        addrs: Option<&IndexMap<AgentId, IpAddr>>,
    ) -> Result<(), DatabaseError> {
        if let Some(addrs) = addrs {
            self.documents.save(
                &AgentDbString::ResolvedAddrs,
                &format::BinaryData(addrs.to_byte_vec()?),
            )
        } else {
            self.documents
                .delete(&AgentDbString::ResolvedAddrs)
                .map(|_| ())
        }
    }

    pub fn env_state(&self) -> Result<Option<EnvState>, DatabaseError> {
        Ok(self
            .documents
            .restore(&AgentDbString::EnvState)?
            .map(|format::BinaryData(bytes)| read_dataformat(&mut bytes.reader()))
            .transpose()?)
    }

    pub fn set_env_state(&self, state: Option<&EnvState>) -> Result<(), DatabaseError> {
        if let Some(state) = state {
            self.documents.save(
                &AgentDbString::EnvState,
                &format::BinaryData(state.to_byte_vec()?),
            )
        } else {
            self.documents.delete(&AgentDbString::EnvState).map(|_| ())
        }
    }

    pub fn last_height(&self) -> Result<Option<(usize, HeightRequest)>, DatabaseError> {
        Ok(
            if let Some(format::BinaryData(bytes)) =
                self.documents.restore(&AgentDbString::LastHeight)?
            {
                let (counter, req) =
                    read_dataformat::<_, (PackedUint, HeightRequest)>(&mut bytes.reader())?;
                Some((counter.into(), req))
            } else {
                None
            },
        )
    }

    pub fn set_last_height(
        &self,
        height: Option<(usize, HeightRequest)>,
    ) -> Result<(), DatabaseError> {
        if let Some((counter, req)) = height {
            self.documents.save(
                &AgentDbString::LastHeight,
                &format::BinaryData((PackedUint::from(counter), req).to_byte_vec()?),
            )
        } else {
            self.documents
                .delete(&AgentDbString::LastHeight)
                .map(|_| ())
        }
    }
}
