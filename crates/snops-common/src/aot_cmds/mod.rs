use std::{io, path::PathBuf};

use tokio::process::{Child, Command};

pub mod error;
pub use error::AotCmdError;

use self::error::CommandError;
use crate::{
    constant::{LEDGER_BASE_DIR, SNARKOS_GENESIS_FILE},
    state::NetworkId,
};

pub struct AotCmd {
    bin: PathBuf,
    network: NetworkId,
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

    pub async fn authorize(
        &self,
        private_key: &str,
        program_id: &str,
        function_name: &str,
        inputs: &[String],
        priority_fee: Option<u64>,
        fee_record: Option<&String>,
    ) -> Result<String, AotCmdError> {
        let mut command = Command::new(&self.bin);
        command
            .stdout(std::io::stdout())
            .stderr(std::io::stderr())
            .env("NETWORK", self.network.to_string())
            .arg("program")
            .arg("authorize")
            .arg("--private-key")
            .arg(private_key);

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
            "aot program authorize",
            Self::parse_string,
        )
    }

    pub async fn authorize_program(
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
            .arg("program")
            .arg("authorize-program")
            .arg("--private-key")
            .arg(private_key)
            .arg(format!("{program_id}/{function_name}"))
            .args(inputs);

        Self::handle_output(
            command.output().await,
            "output",
            "aot program authorize",
            Self::parse_string,
        )
    }

    pub async fn authorize_fee(
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
            .arg("program")
            .arg("authorize-fee")
            .arg("--authorization")
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
            "aot program authorize-fee",
            Self::parse_string,
        )
    }

    pub async fn execute(
        &self,
        func: String,
        fee: Option<String>,
        query: String,
    ) -> Result<String, AotCmdError> {
        let mut command = Command::new(&self.bin);
        command
            .stdout(std::io::stdout())
            .stderr(std::io::stderr())
            .env("NETWORK", self.network.to_string())
            .arg("program")
            .arg("execute")
            .arg("--broadcast")
            .arg("--query")
            .arg(query)
            .arg("--authorization")
            .arg(func);

        if let Some(fee) = fee {
            command.arg("--fee").arg(fee);
        }

        Self::handle_output(
            command.output().await,
            "output",
            "aot program execute",
            Self::parse_string,
        )
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
