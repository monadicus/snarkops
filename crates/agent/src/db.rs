use std::{
    io::{Read, Write},
    path::Path,
    sync::Mutex,
};

use bytes::Buf;
use snops_common::{
    api::EnvInfo,
    db::{error::DatabaseError, tree::DbTree, Database as DatabaseTrait},
    format::{self, read_dataformat, DataFormat, DataReadError, DataWriteError},
    state::{AgentState, EnvId},
};
use url::Url;

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
    /// Latest stored environment info.
    EnvInfo,
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

    pub fn env_info(&self) -> Result<Option<(EnvId, EnvInfo)>, DatabaseError> {
        self.documents
            .restore(&AgentDbString::EnvInfo)?
            .map(|format::BinaryData(bytes)| read_dataformat(&mut bytes.reader()))
            .transpose()
            .map_err(DatabaseError::from)
    }

    pub fn set_env_info(&self, info: Option<&(EnvId, EnvInfo)>) -> Result<(), DatabaseError> {
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

    pub fn set_agent_state(&self, state: Option<&AgentState>) -> Result<(), DatabaseError> {
        if let Some(state) = state {
            self.documents.save(
                &AgentDbString::AgentState,
                &format::BinaryData(state.to_byte_vec()?),
            )
        } else {
            self.documents
                .delete(&AgentDbString::AgentState)
                .map(|_| ())
        }
    }
}
