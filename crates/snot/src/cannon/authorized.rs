use std::path::PathBuf;

use anyhow::{ensure, Result};
use tokio::process::Command;

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
    pub async fn run(self, bin: &PathBuf) -> Result<serde_json::Value> {
        let mut command = Command::new(bin);
        command
            .stdout(std::io::stdout())
            .stderr(std::io::stderr())
            .arg("aot")
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

        let res = command.output().await?;

        let blob: serde_json::Value = serde_json::from_slice(&res.stdout)?;

        ensure!(blob.is_object(), "expected JSON object in response");
        ensure!(
            blob.get("function").is_some()
                && blob.get("fee").is_some()
                && blob.get("broadcast").is_some(),
            "expected function, fee, and broadcast fields in response"
        );

        Ok(blob)
    }
}
