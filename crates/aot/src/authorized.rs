use std::str::FromStr;

use anyhow::{bail, Result};
use clap::Args;
use rand::{CryptoRng, Rng};
use snarkvm::{
    console::program::Network as NetworkTrait,
    ledger::{
        query::Query,
        store::{helpers::memory::ConsensusMemory, ConsensusStore},
    },
    prelude::{
        de, Deserialize, DeserializeExt, Deserializer, Serialize, SerializeStruct, Serializer,
    },
    synthesizer::process::cost_in_microcredits,
    utilities::ToBytes,
};
use tracing::error;

use crate::{
    credits::PROCESS, Aleo, Authorization, DbLedger, MemVM, Network, PrivateKey, Transaction, Value,
};

#[derive(Clone, Debug)]
pub struct Authorized {
    /// The authorization for the main function execution.
    function: Authorization,
    /// The authorization for the fee execution.
    fee: Option<Authorization>,
    /// Whether to broadcast the transaction.
    broadcast: bool,
}

pub enum ExecutionMode<'a> {
    Local(Option<&'a DbLedger>, Option<String>),
    Remote(String),
}

#[derive(Debug, Args)]
pub struct Execute {
    pub authorization: Authorized,
    #[arg(short, long)]
    pub query: String,
}

impl Execute {
    pub fn parse(self) -> Result<()> {
        let broadcast = self.authorization.broadcast;
        // execute the transaction
        let tx = self.authorization.execute_local(
            None,
            &mut rand::thread_rng(),
            Some(self.query.to_owned()),
        )?;

        if !broadcast {
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
            // Return the error.
        } else {
            let status = response.status();
            let err = response.text()?;
            error!("broadcast failed with code {}: {}", status, err);
            bail!(err)
        }
    }
}

impl Authorized {
    /// Initializes a new authorization.
    const fn new(function: Authorization, fee: Option<Authorization>, broadcast: bool) -> Self {
        Self {
            function,
            fee,
            broadcast,
        }
    }

    /// An internal method that authorizes a function call with a corresponding
    /// fee.
    pub fn authorize(
        private_key: &PrivateKey,
        program_id: &str,
        function_name: &str,
        inputs: Vec<Value>,
        priority_fee_in_microcredits: u64,
        broadcast: bool,
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
        // Retrieve the execution ID.
        let execution_id: snarkvm::prelude::Field<Network> = function.to_execution_id()?;
        // Determine the base fee in microcredits.
        let base_fee_in_microcredits = estimate_cost(&function)?;

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
        // Construct the authorization.
        Ok(Self::new(function, fee, broadcast))
    }

    /// Executes the authorization, returning the resulting transaction.
    pub fn execute_remote(self, api_url: &str) -> Result<Transaction> {
        // Execute the authorization.
        let response = reqwest::blocking::Client::new()
            .post(format!("{api_url}/execute"))
            .header("Content-Type", "application/json")
            .json(&self)
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
    ) -> Result<Transaction> {
        // Execute the transaction.
        if let Some(ledger) = ledger {
            let query = query.map(Query::REST);

            ledger
                .vm()
                .execute_authorization(self.function, self.fee, query, rng)
        } else {
            let query = query.map(Query::REST);

            let store = ConsensusStore::<crate::Network, ConsensusMemory<_>>::open(None)?;
            MemVM::from(store)?.execute_authorization(self.function, self.fee, query, rng)
        }
    }

    pub fn execute<R: Rng + CryptoRng>(
        self,
        rng: &mut R,
        mode: ExecutionMode<'_>,
    ) -> Result<Transaction> {
        // Execute the transaction.
        match mode {
            ExecutionMode::Local(ledger, query) => self.execute_local(ledger, rng, query),
            ExecutionMode::Remote(ref api_url) => self.execute_remote(api_url),
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

impl Serialize for Authorized {
    /// Serializes the authorization into string or bytes.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut authorization = serializer.serialize_struct("Authorized", 3)?;
        authorization.serialize_field("function", &self.function)?;
        if let Some(fee) = &self.fee {
            authorization.serialize_field("fee", fee)?;
        }
        authorization.serialize_field("broadcast", &self.broadcast)?;
        authorization.end()
    }
}

impl<'de> Deserialize<'de> for Authorized {
    /// Deserializes the authorization from a string or bytes.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Parse the authorization from a string into a value.
        let mut authorization = serde_json::Value::deserialize(deserializer)?;
        // Retrieve the function authorization.
        let function: Authorization =
            DeserializeExt::take_from_value::<D>(&mut authorization, "function")?;
        // Retrieve the fee authorization, if it exists.
        let fee = serde_json::from_value(
            authorization
                .get_mut("fee")
                .unwrap_or(&mut serde_json::Value::Null)
                .take(),
        )
        .map_err(de::Error::custom)?;
        // Retrieve the broadcast flag.
        let broadcast = DeserializeExt::take_from_value::<D>(&mut authorization, "broadcast")?;
        // Recover the authorization.
        Ok(Self {
            function,
            fee,
            broadcast,
        })
    }
}

impl FromStr for Authorized {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        serde_json::from_str(s).map_err(anyhow::Error::from)
    }
}
