use std::{io, path::PathBuf, process::Stdio};

use tokio::{
    io::AsyncWriteExt,
    process::{Child, Command},
};

pub mod error;
pub use error::AotCmdError;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use self::error::CommandError;
use crate::{
    constant::{LEDGER_BASE_DIR, SNARKOS_GENESIS_FILE},
    state::NetworkId,
};

pub struct AotCmd {
    bin: PathBuf,
    network: NetworkId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Authorization {
    Program {
        auth: Value,
        fee_auth: Option<Value>,
    },
    Deploy {
        owner: Value,
        deployment: Value,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        fee_auth: Option<Value>,
    },
}

type Output = io::Result<std::process::Output>;
impl AotCmd {
    pub fn new(bin: PathBuf, network: NetworkId) -> Self {
        Self { bin, network }
    }

    fn handle_output<T, F>(
        output: Output,
        act: &'static str,
        cmd: &'static str,
        parse: F,
    ) -> Result<T, AotCmdError>
    where
        F: Fn(Vec<u8>) -> Result<T, AotCmdError>,
    {
        let res = output.map_err(|e| AotCmdError::Command(CommandError::action(act, cmd, e)))?;

        if !res.status.success() {
            Err(AotCmdError::Command(CommandError::status(
                cmd,
                res.status,
                String::from_utf8_lossy(&res.stderr).to_string(),
            )))?;
        }

        let parsed_output = parse(res.stdout)?;
        Ok(parsed_output)
    }

    fn parse_string(bytes: Vec<u8>) -> Result<String, AotCmdError> {
        Ok(unsafe { String::from_utf8_unchecked(bytes) })
    }

    fn _parse_string_option(bytes: Vec<u8>) -> Result<Option<String>, AotCmdError> {
        let string = unsafe { String::from_utf8_unchecked(bytes) };
        Ok(if string.is_empty() {
            None
        } else {
            Some(string)
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn authorize_program(
        &self,
        private_key: &str,
        fee_private_key: Option<&String>,
        program_id: &str,
        function_name: &str,
        inputs: &[String],
        query: Option<&String>,
        priority_fee: Option<u64>,
        fee_record: Option<&String>,
    ) -> Result<String, AotCmdError> {
        let mut command = Command::new(&self.bin);
        command
            .stdout(std::io::stdout())
            .stderr(std::io::stderr())
            .env("NETWORK", self.network.to_string())
            .arg("auth")
            .arg("program")
            .arg("--private-key")
            .arg(private_key);

        if let Some(query) = query {
            command.arg("--query").arg(query);
        }

        if let Some(fee_private_key) = fee_private_key {
            command.arg("--fee-private-key").arg(fee_private_key);
        }

        if let Some(priority_fee) = priority_fee {
            command.arg("--priority-fee").arg(priority_fee.to_string());
        }

        if let Some(fee_record) = fee_record {
            command.arg("--record").arg(fee_record);
        }

        command
            .arg(format!("{program_id}/{function_name}"))
            .args(inputs);

        Self::handle_output(
            command.output().await,
            "output",
            "aot auth program",
            Self::parse_string,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn authorize_deploy(
        &self,
        private_key: &str,
        fee_private_key: Option<&String>,
        program: &str,
        query: Option<&String>,
        priority_fee: Option<u64>,
        fee_record: Option<&String>,
    ) -> Result<String, AotCmdError> {
        let mut command = Command::new(&self.bin);
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(std::io::stderr())
            .env("NETWORK", self.network.to_string())
            .arg("auth")
            .arg("deploy")
            .arg("--private-key")
            .arg(private_key);

        if let Some(query) = query {
            command.arg("--query").arg(query);
        }

        if let Some(fee_private_key) = fee_private_key {
            command.arg("--fee-private-key").arg(fee_private_key);
        }

        if let Some(priority_fee) = priority_fee {
            command.arg("--priority-fee").arg(priority_fee.to_string());
        }

        if let Some(fee_record) = fee_record {
            command.arg("--record").arg(fee_record);
        }

        command.arg("-");

        let mut child = command
            .spawn()
            .map_err(|e| CommandError::action("spawning", "aot auth deploy", e))?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(program.as_bytes())
                .await
                .map_err(|e| CommandError::action("writing to", "aot auth deploy stdin", e))?;
        }

        Self::handle_output(
            child.wait_with_output().await,
            "output",
            "aot auth deploy",
            Self::parse_string,
        )
    }

    pub async fn authorize_program_only(
        &self,
        private_key: &str,
        program_id: &str,
        function_name: &str,
        inputs: &[String],
    ) -> Result<String, AotCmdError> {
        let mut command = Command::new(&self.bin);
        command
            .stdout(std::io::stdout())
            .stderr(std::io::stderr())
            .env("NETWORK", self.network.to_string())
            .arg("auth")
            .arg("program")
            .arg("--private-key")
            .arg(private_key)
            .arg(format!("{program_id}/{function_name}"))
            .args(inputs);

        Self::handle_output(
            command.output().await,
            "output",
            "aot auth program",
            Self::parse_string,
        )
    }

    pub async fn authorize_program_fee(
        &self,
        private_key: &str,
        authorization: &str,
        priority_fee: Option<u64>,
        fee_record: Option<&String>,
    ) -> Result<String, AotCmdError> {
        let mut command = Command::new(&self.bin);
        command
            .stdout(std::io::stdout())
            .stderr(std::io::stderr())
            .env("NETWORK", self.network.to_string())
            .arg("auth")
            .arg("fee")
            .arg("--auth")
            .arg(authorization)
            .arg("--private-key")
            .arg(private_key)
            .arg("--priority-fee")
            .arg(priority_fee.unwrap_or_default().to_string());

        if let Some(fee_record) = fee_record {
            command.arg("--record").arg(fee_record);
        }

        Self::handle_output(
            command.output().await,
            "output",
            "aot auth fee",
            Self::parse_string,
        )
    }

    pub async fn execute(&self, auth: Authorization, query: String) -> Result<String, AotCmdError> {
        let mut command = Command::new(&self.bin);
        command
            .env("NETWORK", self.network.to_string())
            .arg("auth")
            .arg("execute")
            .arg("--broadcast")
            .arg("--query")
            .arg(query);

        match auth {
            Authorization::Program { auth, fee_auth } => {
                command.arg("--auth").arg(auth.to_string());
                if let Some(fee_auth) = fee_auth {
                    command.arg("--fee-auth").arg(fee_auth.to_string());
                }
            }
            Authorization::Deploy {
                owner,
                deployment,
                fee_auth,
            } => {
                command.arg("--owner").arg(owner.to_string());
                command.arg("--deployment").arg(deployment.to_string());
                if let Some(fee_auth) = fee_auth {
                    command.arg("--fee-auth").arg(fee_auth.to_string());
                }
            }
        }

        Self::handle_output(
            command.output().await,
            "output",
            "aot auth execute",
            Self::parse_string,
        )
    }

    pub async fn get_tx_id(&self, auth: &Authorization) -> Result<String, AotCmdError> {
        let mut command = Command::new(&self.bin);
        command
            .env("NETWORK", self.network.to_string())
            .arg("auth")
            .arg("id")
            .arg(serde_json::to_string(auth).map_err(AotCmdError::Json)?);

        Self::handle_output(
            command.output().await,
            "output",
            "aot auth id",
            Self::parse_string,
        )
        .map(|s| s.trim().to_string())
    }

    pub fn ledger_query(&self, storage_path: PathBuf, port: u16) -> Result<Child, CommandError> {
        let mut command = Command::new(&self.bin);
        command
            .kill_on_drop(true)
            .stdout(std::io::stdout())
            .stderr(std::io::stderr())
            .env("NETWORK", self.network.to_string())
            .arg("ledger")
            .arg("-l")
            .arg(storage_path.join(LEDGER_BASE_DIR))
            .arg("-g")
            .arg(storage_path.join(SNARKOS_GENESIS_FILE))
            .arg("query")
            .arg("--port")
            .arg(port.to_string())
            .arg("--bind")
            .arg("127.0.0.1") // only bind to localhost as this is a private process
            .arg("--readonly");

        let child = command
            .spawn()
            .map_err(|e| CommandError::action("spawning", "aot ledger", e))?;
        Ok(child)
    }
}
