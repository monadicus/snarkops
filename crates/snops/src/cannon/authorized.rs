use std::path::PathBuf;

use tokio::process::Command;

use super::error::{AuthorizeError, CannonError};
use crate::error::CommandError;

#[derive(Clone, Debug)]
pub enum Authorize {
    TransferPublic {
        private_key: String,
        recipient: String,
        amount: u64,
    },
    Other {
        private_key: String,
        program_id: String,
        function_name: String,
        inputs: Vec<String>,
    },
}

impl Authorize {
    pub async fn run(self, bin: &PathBuf) -> Result<serde_json::Value, CannonError> {
        let mut command = Command::new(bin);
        command
            .stdout(std::io::stdout())
            .stderr(std::io::stderr())
            .arg("program")
            .arg("authorize");

        match self {
            Self::TransferPublic {
                private_key,
                recipient,
                amount,
            } => {
                command
                    .arg("--private-key")
                    .arg(private_key)
                    .arg("--program-id")
                    .arg("credits.aleo")
                    .arg("--function-name")
                    .arg("transfer-public")
                    .arg("--inputs")
                    .args([recipient, format!("{amount}u64")]);
            }
            Self::Other {
                private_key,
                program_id,
                function_name,
                inputs,
            } => {
                command
                    .arg("other")
                    .arg("--private-key")
                    .arg(private_key)
                    .arg("--program-id")
                    .arg(program_id)
                    .arg("--function-name")
                    .arg(function_name)
                    .arg("--inputs")
                    .args(inputs);
            }
        }

        // command.arg("--broadcast");

        let res = command.output().await.map_err(|e| {
            AuthorizeError::Command(CommandError::action("output", "aot authorize", e))
        })?;

        if !res.status.success() {
            Err(AuthorizeError::Command(CommandError::status(
                "aot authorize",
                res.status,
                String::from_utf8_lossy(&res.stderr).to_string(),
            )))?;
        }

        let blob: serde_json::Value =
            serde_json::from_slice(&res.stdout).map_err(AuthorizeError::Json)?;

        dbg!(&blob);

        if !blob.is_object() {
            Err(AuthorizeError::JsonNotObject)?;
        }

        Ok(blob)
    }
}
