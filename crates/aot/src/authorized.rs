use std::str::FromStr;

use anyhow::{bail, Ok, Result};
use clap::{Args, ValueEnum};
use rand::{CryptoRng, Rng};
use serde_json::json;
use snarkvm::{
    console::program::Network as NetworkTrait,
    ledger::{
        query::Query,
        store::{helpers::memory::ConsensusMemory, ConsensusStore},
    },
    prelude::{Deserialize, Serialize},
    synthesizer::process::cost_in_microcredits,
    utilities::ToBytes,
};
use tracing::error;

// use tracing::error;
use crate::{
    credits::PROCESS, Aleo, Authorization, DbLedger, MemVM, Network, PTRecord, PrivateKey,
    Transaction, Value,
};

// TODO: This doesn't really need to be it's own struct anymore
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Authorized {
    /// The authorization for the main function execution.
    function: Authorization,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum FeeMode {
    Public,
    Private,
}

#[derive(Debug, Args)]
pub struct AuthorizeFee {
    #[arg(short, long)]
    pub private_key: PrivateKey,
    /// The Authorization for the function.
    #[arg(short, long)]
    pub authorization: Authorized,
    /// The fee mode: Public or Privete,
    #[arg(short, long, value_enum, default_value_t = FeeMode::Public)]
    pub fee_mode: FeeMode,
    /// The priority fee in microcredits.
    #[clap(long, default_value_t = 0)]
    pub priority_fee: u64,
    /// The record for a private fee.
    #[clap(long)]
    pub record: Option<PTRecord>,
}

impl AuthorizeFee {
    pub fn parse(self) -> Result<Option<Authorization>> {
        let fee = match self.fee_mode {
            FeeMode::Public => self.authorization.fee_public(
                &self.private_key,
                self.priority_fee,
                &mut rand::thread_rng(),
            )?,
            FeeMode::Private if self.record.is_some() => self.authorization.fee_private(
                &self.private_key,
                self.record.unwrap(),
                self.priority_fee,
                &mut rand::thread_rng(),
            )?,
            FeeMode::Private => {
                bail!("A private fee requires a record")
            }
        };

        Ok(fee)
    }
}

#[derive(Debug, Clone, ValueEnum)]
pub enum ExecMode {
    Local,
    Remote,
}

#[derive(Debug, Args)]
pub struct Execute {
    /// The Authorization for the function.
    #[arg(short, long)]
    pub authorization: Authorized,
    #[arg(short, long, value_enum, default_value_t = ExecMode::Local)]
    pub exec_mode: ExecMode,
    #[arg(short, long)]
    pub query: String,
    /// The authorization for the fee execution.
    #[arg(short, long)]
    pub fee: Option<Authorization>,
    /// Whether to broadcast the transaction.
    #[arg(short, long, default_value_t = false)]
    pub broadcast: bool,
}

impl Execute {
    pub fn parse(self) -> Result<()> {
        // execute the transaction
        let tx = match self.exec_mode {
            ExecMode::Local => self.authorization.execute_local(
                None,
                &mut rand::thread_rng(),
                Some(self.query.to_owned()),
                self.fee,
            ),
            ExecMode::Remote => self.authorization.execute_remote(&self.query, self.fee),
        }?;

        if !self.broadcast {
            println!("{}", serde_json::to_string(&tx)?);
            return Ok(());
        }

        // Broadcast the transaction.
        tracing::info!("broadcasting transaction...");
        tracing::debug!("{}", serde_json::to_string(&tx)?);
        let response = reqwest::blocking::Client::new()
            .post(format!("{}/mainnet/transaction/broadcast", self.query))
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

impl Authorized {
    /// Initializes a new authorization.
    const fn new(function: Authorization) -> Self {
        Self { function }
    }

    /// A method that authorizes a function call with a corresponding
    /// fee.
    pub fn authorize(
        private_key: &PrivateKey,
        program_id: &str,
        function_name: &str,
        inputs: Vec<Value>,
        rng: &mut (impl Rng + CryptoRng),
    ) -> Result<Authorized> {
        // Authorize the main function.
        let function = PROCESS.authorize::<Aleo, _>(
            private_key,
            program_id,
            function_name,
            inputs.into_iter(),
            rng,
        )?;

        // Construct the authorization.
        Ok(Self::new(function))
    }

    pub fn fee_public(
        &self,
        private_key: &PrivateKey,
        priority_fee_in_microcredits: u64,
        rng: &mut (impl Rng + CryptoRng),
    ) -> Result<Option<Authorization>> {
        // Retrieve the execution ID.
        let execution_id: snarkvm::prelude::Field<Network> = self.function.to_execution_id()?;
        // Determine the base fee in microcredits.
        let base_fee_in_microcredits = estimate_cost(&self.function)?;

        // Authorize the fee.
        let fee = match base_fee_in_microcredits == 0 && priority_fee_in_microcredits == 0 {
            true => None,
            false => Some(PROCESS.authorize_fee_public::<Aleo, _>(
                private_key,
                base_fee_in_microcredits,
                priority_fee_in_microcredits,
                execution_id,
                rng,
            )?),
        };

        Ok(fee)
    }

    pub fn fee_private(
        &self,
        private_key: &PrivateKey,
        credits: PTRecord,
        priority_fee_in_microcredits: u64,
        rng: &mut (impl Rng + CryptoRng),
    ) -> Result<Option<Authorization>> {
        // Retrieve the execution ID.
        let execution_id: snarkvm::prelude::Field<Network> = self.function.to_execution_id()?;
        // Determine the base fee in microcredits.
        let base_fee_in_microcredits = estimate_cost(&self.function)?;

        // Authorize the fee.
        let fee = match base_fee_in_microcredits == 0 && priority_fee_in_microcredits == 0 {
            true => None,
            false => Some(PROCESS.authorize_fee_private::<Aleo, _>(
                private_key,
                credits,
                base_fee_in_microcredits,
                priority_fee_in_microcredits,
                execution_id,
                rng,
            )?),
        };

        Ok(fee)
    }

    /// Executes the authorization, returning the resulting transaction.
    pub fn execute_remote(self, api_url: &str, fee: Option<Authorization>) -> Result<Transaction> {
        // Execute the authorization.
        let response = reqwest::blocking::Client::new()
            .post(format!("{api_url}/execute"))
            .header("Content-Type", "application/json")
            // not actually sure this is how we send the fee?
            .json(&json!({
                "authorization": self.function,
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
    pub fn execute_local<R: Rng + CryptoRng>(
        self,
        ledger: Option<&DbLedger>,
        rng: &mut R,
        query: Option<String>,
        fee: Option<Authorization>,
    ) -> Result<Transaction> {
        // Execute the transaction.
        if let Some(ledger) = ledger {
            let query = query.map(Query::REST);

            ledger
                .vm()
                .execute_authorization(self.function, fee, query, rng)
        } else {
            let query = query.map(Query::REST);

            let store = ConsensusStore::<crate::Network, ConsensusMemory<_>>::open(None)?;
            MemVM::from(store)?.execute_authorization(self.function, fee, query, rng)
        }
    }
}

fn estimate_cost(func: &Authorization) -> Result<u64> {
    let transitions = func.transitions();

    let storage_cost = {
        let mut cost = 0u64;

        cost += 1; // execution version, 1 byte
        cost += 1; // number of transitions, 1 byte

        // write each transition
        for transition in transitions.values() {
            cost += transition.to_bytes_le()?.len() as u64;
        }

        // state root (this is 32 bytes)
        cost += <Network as NetworkTrait>::StateRoot::default()
            .to_bytes_le()?
            .len() as u64;

        // proof option is_some (1 byte)
        cost += 1;
        // Proof<Network> version
        cost += 1;

        cost += 956; // size of proof with 1 batch size

        /* cost += varuna::Proof::<<Network as Environment>::PairingCurve>::new(
            todo!("batch_sizes"),
            todo!("commitments"),
            todo!("evaluations"),
            todo!("prover_third_message"),
            todo!("prover_fourth_message"),
            todo!("pc_proof"),
        )?
        .to_bytes_le()?
        .len() as u64; */

        cost
    };
    //execution.size_in_bytes().map_err(|e| e.to_string())?;

    // Compute the finalize cost in microcredits.
    let mut finalize_cost = 0u64;
    // Iterate over the transitions to accumulate the finalize cost.
    for (_key, transition) in transitions {
        // Retrieve the function name, program id, and program.
        let function_name = transition.function_name();
        let stack = PROCESS.get_stack(transition.program_id())?;
        let cost = cost_in_microcredits(stack, function_name)?;

        // Accumulate the finalize cost.
        if let Some(cost) = finalize_cost.checked_add(cost) {
            finalize_cost = cost;
        } else {
            bail!("The finalize cost computation overflowed for an execution")
        };
    }
    Ok(storage_cost + finalize_cost)
}

impl FromStr for Authorized {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        serde_json::from_str(s).map_err(anyhow::Error::from)
    }
}
