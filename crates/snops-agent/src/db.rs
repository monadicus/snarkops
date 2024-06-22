use std::{
    io::{Read, Write},
    path::Path,
    sync::Mutex,
};

use snops_common::{
    db::{error::DatabaseError, tree::DbTree, Database as DatabaseTrait},
    format::{DataFormat, DataReadError, DataWriteError},
};

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
#[repr(u8)]
pub enum AgentDbString {
    /// JSON web token of agent.
    Jwt,
    /// Process ID of node. Used to keep track of zombie node processes.
    NodePid,
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
    pub pid_mutex: tokio::sync::Mutex<Option<u32>>,
    pub strings: DbTree<AgentDbString, String>,
}

impl DatabaseTrait for Database {
    fn open<P: AsRef<Path>>(path: P) -> Result<Self, DatabaseError> {
        let db = sled::open(path)?;
        let strings = DbTree::new(db.open_tree(b"v1/strings")?);
        let jwt_mutex = Mutex::new(strings.restore(&AgentDbString::Jwt)?);
        let pid_mutex = tokio::sync::Mutex::new(
            strings
                .restore(&AgentDbString::NodePid)?
                .map(|i: String| i.parse().unwrap()),
        );

        Ok(Self {
            db,
            jwt_mutex,
            pid_mutex,
            strings,
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

    pub async fn set_pid(&self, pid: Option<u32>) -> Result<(), DatabaseError> {
        let mut lock = self.pid_mutex.lock().await;
        self.strings
            .save_option(&AgentDbString::NodePid, pid.map(|p| p.to_string()).as_ref())?;
        *lock = pid;
        Ok(())
    }
}
