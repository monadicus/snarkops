use anyhow::{anyhow, bail, Result};
use clap::{Args, ValueEnum};
use rand::{CryptoRng, Rng};
use snarkvm::ledger::{
    query::Query,
    store::{helpers::memory::ConsensusMemory, ConsensusStore},
};
use tracing::error;

use super::{args::AuthArgs, query};
use crate::{
    auth::args::AuthBlob, Authorization, DbLedger, MemVM, Network, NetworkId, Transaction,
};

#[derive(Debug, Clone, ValueEnum)]
pub enum ExecMode {
    Local,
    Remote,
}

/// A command to execute an authorization.
#[derive(Debug, Args)]
pub struct Execute<N: Network> {
    /// The execution mode: local(local ledgr) or remote(api to another node).
    #[arg(short, long, value_enum, default_value_t = ExecMode::Local)]
    pub exec_mode: ExecMode,
    /// Query endpoint.
    #[arg(short, long)]
    pub query: String,
    /// Whether to broadcast the transaction.
    #[arg(short, long, default_value_t = false)]
    pub broadcast: bool,
    /// The Authorization for the function.
    #[clap(flatten)]
    pub auth: AuthArgs<N>,
}

/// Executes the authorization remotely
pub fn execute_remote<N: Network>(api_url: &str, auth: AuthBlob<N>) -> Result<()> {
    // Execute the authorization.
    let response = reqwest::blocking::Client::new()
        .post(format!("{api_url}/auth"))
        .header("Content-Type", "application/json")
        // not actually sure this is how we send the fee?
        .json(&auth)
        .send()?;

    // TODO: this can properly return the transaction once snops auth proxy monitors
    // tx ids

    // Ensure the response is successful.
    match response.status().is_success() {
        // Return the transaction.
        true => Ok(()),
        // Return the error.
        false => bail!(response.text()?),
    }
}

/// Executes the authorization locally, returning the resulting transaction.
pub fn execute_local<R: Rng + CryptoRng, N: Network>(
    auth: AuthBlob<N>,
    ledger: Option<&DbLedger<N>>,
    query_raw: Option<String>,
    rng: &mut R,
) -> Result<Transaction<N>> {
    // Execute the transaction.
    if let Some(ledger) = ledger {
        let query = query_raw.map(Query::REST);

        match auth {
            AuthBlob::Program { auth, fee_auth } => {
                ledger
                    .vm()
                    .execute_authorization(auth.into(), fee_auth.map(Into::into), query, rng)
            }
            AuthBlob::Deploy {
                deployment,
                owner,
                fee_auth,
            } => {
                let fee = ledger.vm().execute_fee_authorization(
                    fee_auth
                        .map(Into::into)
                        .ok_or(anyhow!("expected fee for deployment"))?,
                    query,
                    rng,
                )?;
                Ok(Transaction::from_deployment(owner, deployment, fee)?)
            }
        }
    } else {
        let query = query_raw.clone().map(Query::REST);

        let store = ConsensusStore::<N, ConsensusMemory<_>>::open(None)?;
        let vm = MemVM::from(store)?;

        match auth {
            AuthBlob::Program { auth, fee_auth } => {
                let auth: Authorization<N> = auth.into();
                let fee_auth: Option<Authorization<N>> = fee_auth.map(Into::into);

                {
                    let guard = vm.process();
                    let process = &mut *guard.write();
                    if let Some(query_raw) = query_raw.as_deref() {
                        let programs = query::get_programs_from_auth(&auth);
                        query::add_many_programs_to_process(process, programs, query_raw)?;
                    }
                }

                vm.execute_authorization(auth, fee_auth, query, rng)
            }
            AuthBlob::Deploy {
                deployment,
                owner,
                fee_auth,
            } => {
                let fee_auth: Option<Authorization<N>> = fee_auth.map(Into::into);

                let fee = vm.execute_fee_authorization(
                    fee_auth.ok_or(anyhow!("expected fee for deployment"))?,
                    query,
                    rng,
                )?;
                Ok(Transaction::from_deployment(owner, deployment, fee)?)
            }
        }
    }
}

impl<N: Network> Execute<N> {
    pub fn parse(self) -> Result<()> {
        // execute the transaction
        let tx = match self.exec_mode {
            ExecMode::Local => execute_local(
                self.auth.pick()?,
                None,
                Some(self.query.to_owned()),
                &mut rand::thread_rng(),
            )?,
            ExecMode::Remote => return execute_remote(&self.query, self.auth.pick()?),
        };

        if !self.broadcast {
            println!("{}", serde_json::to_string(&tx)?);
            return Ok(());
        }

        let network = NetworkId::from_network::<N>();

        // Broadcast the transaction.
        tracing::info!("broadcasting transaction...");
        println!("{}", serde_json::to_string(&tx)?);
        let response = reqwest::blocking::Client::new()
            .post(format!("{}/{network}/transaction/broadcast", self.query))
            .header("Content-Type", "application/json")
            .json(&tx)
            .send()?;

        // Ensure the response is successful.
        if response.status().is_success() {
            // Return the transaction.
            // println!("{}", response.text()?);
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
