use anyhow::{bail, Result};
use clap::{Args, ValueEnum};
use rand::{CryptoRng, Rng};
use serde_json::json;
use snarkvm::{
    console::program::Network,
    ledger::{
        query::Query,
        store::{helpers::memory::ConsensusMemory, ConsensusStore},
    },
};
use tracing::error;

use crate::{Authorization, DbLedger, MemVM, NetworkId, Transaction};

#[derive(Debug, Clone, ValueEnum)]
pub enum ExecMode {
    Local,
    Remote,
}

#[derive(Debug, Args)]
pub struct Execute<N: Network> {
    /// The Authorization for the function.
    #[arg(short, long)]
    pub authorization: Authorization<N>,
    #[arg(short, long, value_enum, default_value_t = ExecMode::Local)]
    pub exec_mode: ExecMode,
    #[arg(short, long)]
    pub query: String,
    /// The authorization for the fee execution.
    #[arg(short, long)]
    pub fee: Option<Authorization<N>>,
    /// Whether to broadcast the transaction.
    #[arg(short, long, default_value_t = false)]
    pub broadcast: bool,
}

/// Executes the authorization, returning the resulting transaction.
pub fn execute_remote<N: Network>(
    auth: Authorization<N>,
    api_url: &str,
    fee: Option<Authorization<N>>,
) -> Result<Transaction<N>> {
    // Execute the authorization.
    let response = reqwest::blocking::Client::new()
        .post(format!("{api_url}/execute"))
        .header("Content-Type", "application/json")
        // not actually sure this is how we send the fee?
        .json(&json!({
                "authorization": auth,
                "fee": fee,
        }))
        .send()?;

    // Ensure the response is successful.
    match response.status().is_success() {
        // Return the transaction.
        true => Ok(response.json()?),
        // Return the error.
        false => bail!(response.text()?),
    }
}

/// Executes the authorization locally, returning the resulting transaction.
pub fn execute_local<R: Rng + CryptoRng, N: Network>(
    auth: Authorization<N>,
    ledger: Option<&DbLedger<N>>,
    rng: &mut R,
    query: Option<String>,
    fee: Option<Authorization<N>>,
) -> Result<Transaction<N>> {
    // Execute the transaction.
    if let Some(ledger) = ledger {
        let query = query.map(Query::REST);

        ledger.vm().execute_authorization(auth, fee, query, rng)
    } else {
        let query = query.map(Query::REST);

        let store = ConsensusStore::<N, ConsensusMemory<_>>::open(None)?;
        MemVM::from(store)?.execute_authorization(auth, fee, query, rng)
    }
}

impl<N: Network> Execute<N> {
    pub fn parse(self) -> Result<()> {
        // execute the transaction
        let tx = match self.exec_mode {
            ExecMode::Local => execute_local(
                self.authorization,
                None,
                &mut rand::thread_rng(),
                Some(self.query.to_owned()),
                self.fee,
            ),
            ExecMode::Remote => execute_remote(self.authorization, &self.query, self.fee),
        }?;

        if !self.broadcast {
            println!("{}", serde_json::to_string(&tx)?);
            return Ok(());
        }

        let network = NetworkId::from_network::<N>();

        // Broadcast the transaction.
        tracing::info!("broadcasting transaction...");
        tracing::debug!("{}", serde_json::to_string(&tx)?);
        let response = reqwest::blocking::Client::new()
            .post(format!("{}/{network}/transaction/broadcast", self.query))
            .header("Content-Type", "application/json")
            .json(&tx)
            .send()?;

        // Ensure the response is successful.
        if response.status().is_success() {
            // Return the transaction.
            println!("{}", response.text()?);
            Ok(())
        } else {
            // Return the error.
            let status = response.status();
            let err = response.text()?;
            error!("broadcast failed with code {}: {}", status, err);
            bail!(err)
        }
    }
}
