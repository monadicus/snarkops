use std::{
    io::{Read, Write},
    net::IpAddr,
    path::Path,
    sync::{Arc, Mutex},
};

use indexmap::IndexMap;
use snops_common::{
    api::AgentEnvInfo,
    db::{
        error::DatabaseError,
        tree::{DbRecords, DbTree},
        Database as DatabaseTrait,
    },
    format::{DataFormat, DataReadError, DataWriteError, PackedUint},
    state::{AgentId, AgentState, EnvId, HeightRequest},
};
use url::Url;

use crate::reconcile::state::EnvState;

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
#[repr(u8)]
pub enum AgentDbString {
    /// JSON web token of agent.
    Jwt = 0,
    /// Process ID of node. Used to keep track of zombie node processes.
    NodePid = 1,
    // Url to Loki instance, configured by the endpoint.
    LokiUrl = 2,
    /// Current state of the agent.
    AgentState = 3,
    /// Current environment state.
    EnvState = 4,
    /// Latest stored environment info.
    EnvInfo = 5,
    /// Agent addresses resolved by the controlplane.
    ResolvedAddrs = 6,
    /// Last height of the agent state
    LastHeight = 7,
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
            5 => Self::EnvState,
            6 => Self::ResolvedAddrs,
            7 => Self::LastHeight,
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
    pub documents: DbRecords<AgentDbString>,
}

impl DatabaseTrait for Database {
    fn open(path: &Path) -> Result<Self, DatabaseError> {
        let db = sled::open(path)?;
        let strings = DbTree::new(db.open_tree(b"v1/strings")?);
        let documents = DbRecords::new(db.open_tree(b"v1/documents")?);
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

    pub fn env_info(&self) -> Result<Option<(EnvId, Arc<AgentEnvInfo>)>, DatabaseError> {
        self.documents
            .restore(&AgentDbString::EnvInfo)
            .map_err(DatabaseError::from)
    }

    pub fn set_env_info(
        &self,
        info: Option<(EnvId, Arc<AgentEnvInfo>)>,
    ) -> Result<(), DatabaseError> {
        self.documents
            .save_option(&AgentDbString::EnvInfo, info.as_ref())
    }

    pub fn agent_state(&self) -> Result<AgentState, DatabaseError> {
        Ok(self
            .documents
            .restore(&AgentDbString::AgentState)?
            .unwrap_or_default())
    }

    pub fn set_agent_state(&self, state: &AgentState) -> Result<(), DatabaseError> {
        self.documents.save(&AgentDbString::AgentState, state)
    }

    pub fn resolved_addrs(&self) -> Result<IndexMap<AgentId, IpAddr>, DatabaseError> {
        Ok(self
            .documents
            .restore(&AgentDbString::ResolvedAddrs)?
            .unwrap_or_default())
    }

    pub fn set_resolved_addrs(
        &self,
        addrs: Option<&IndexMap<AgentId, IpAddr>>,
    ) -> Result<(), DatabaseError> {
        self.documents
            .save_option(&AgentDbString::ResolvedAddrs, addrs)
    }

    pub fn env_state(&self) -> Result<Option<EnvState>, DatabaseError> {
        self.documents.restore(&AgentDbString::EnvState)
    }

    pub fn set_env_state(&self, state: Option<&EnvState>) -> Result<(), DatabaseError> {
        self.documents.save_option(&AgentDbString::EnvState, state)
    }

    pub fn last_height(&self) -> Result<Option<(usize, HeightRequest)>, DatabaseError> {
        Ok(self
            .documents
            .restore::<(PackedUint, HeightRequest)>(&AgentDbString::LastHeight)?
            .map(|(counter, req)| (counter.into(), req)))
    }

    pub fn set_last_height(
        &self,
        height: Option<(usize, HeightRequest)>,
    ) -> Result<(), DatabaseError> {
        self.documents.save_option(
            &AgentDbString::LastHeight,
            height
                .map(|(counter, req)| (PackedUint::from(counter), req))
                .as_ref(),
        )
    }
}
