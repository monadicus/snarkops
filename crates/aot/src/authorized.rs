use std::str::FromStr;

use anyhow::{bail, Result};
use clap::Args;
use rand::{CryptoRng, Rng};
use snarkvm::{
    ledger::{
        query::Query,
        store::{helpers::memory::ConsensusMemory, ConsensusStore},
    },
    prelude::{
        de, Deserialize, DeserializeExt, Deserializer, Serialize, SerializeStruct, Serializer,
    },
};

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
    pub query: Option<String>,
}

impl Execute {
    pub fn parse(self) -> Result<()> {
        Ok(())
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
        let base_fee_in_microcredits = 0;
        // get_base_fee_in_microcredits(program_id, function_name)?;
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
            .body(serde_json::to_string(&self)?)
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
