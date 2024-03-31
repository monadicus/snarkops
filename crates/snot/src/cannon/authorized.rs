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
        priority_fee: u64,
    },
}

impl Authorize {
    pub async fn run(self, bin: &PathBuf) -> Result<serde_json::Value, CannonError> {
        let mut command = Command::new(bin);
        command
            .stdout(std::io::stdout())
            .stderr(std::io::stderr())
            .arg("authorize");

        match self {
            Self::TransferPublic {
                private_key,
                recipient,
                amount,
                priority_fee,
            } => {
                command
                    .arg("transfer-public")
                    .arg("--private-key")
                    .arg(private_key)
                    .arg("--recipient")
                    .arg(recipient)
                    .arg("--amount")
                    .arg(amount.to_string())
                    .arg("--priority-fee")
                    .arg(priority_fee.to_string());
            }
        }

        command.arg("--broadcast");

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

        // TODO consider making a type for this json object
        if !blob.is_object() {
            Err(AuthorizeError::JsonNotObject)?;
        }

        if blob.get("function").is_none()
            || blob.get("broadcast").is_none()
            || blob.get("fee").is_none()
        {
            Err(AuthorizeError::InvalidJson)?;
        }

        Ok(blob)
    }
}
