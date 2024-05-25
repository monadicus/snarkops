use std::{io, path::PathBuf};

use tokio::process::Command;

pub mod error;
pub use error::AotCmdError;

use self::error::CommandError;
use crate::state::NetworkId;

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

        let pasred_output = parse(res.stdout)?;
        Ok(pasred_output)
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
            .arg("--authorization")
            .arg(&func)
            .arg("--exec-mode")
            // hard coded for now since this is all we used
            .arg("local")
            .arg("--query")
            .arg(query)
            .arg("--brodcast")
            // hard coded for now since this is all we used
            .arg(true.to_string());

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
}
