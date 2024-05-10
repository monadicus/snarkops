// TODO should rename this file now...
use anyhow::Result;
use clap::Args;
use snarkvm::synthesizer::Process;

use crate::{authorized::Authorized, Network, PrivateKey, Value};

lazy_static::lazy_static! {
    /// The main process.
    pub(crate) static ref PROCESS: Process<Network> = Process::load().unwrap();
}
#[derive(Clone, Debug, Args)]
pub struct Authorize {
    #[clap(long)]
    private_key: PrivateKey,
    #[clap(long)]
    program_id: String,
    #[clap(long)]
    function_name: String,
    #[clap(long)]
    inputs: Vec<Value>,
}

impl Authorize {
    /// Initializes a new authorization.
    pub fn parse(self) -> Result<Authorized> {
        Authorized::authorize(
            &self.private_key,
            &self.program_id,
            &self.function_name,
            self.inputs,
            &mut rand::thread_rng(),
        )
    }
}

// TODO: should we keep these so we can have nicer syntax for them
// in the timeline yaml? probably more of a pain tbh
// pub struct Credits;

// impl Credits {
//     /// Returns a transaction that allows any staker to bond their
// microcredits     /// to a validator.
//     pub fn bond_public(
//         private_key: &str,
//         validator: &str,
//         amount_in_microcredits: u64,
//         priority_fee_in_microcredits: u64,
//         broadcast: bool,
//         rng: &mut (impl Rng + CryptoRng),
//     ) -> Result<Authorized> {
//         // Initialize the private key.
//         let private_key = PrivateKey::from_str(private_key)?;
//         // Initialize the validator's address.
//         let validator = Address::from_str(validator)?;
//         // Initialize the amount in microcredits.
//         let amount_in_microcredits = U64::new(amount_in_microcredits);

//         // Construct the program ID and function name.
//         let (program_id, function_name) = ("credits.aleo", "bond_public");
//         // Construct the inputs.
//         let inputs = vec![
//             Value::from(Literal::Address(validator)),
//             Value::from(Literal::U64(amount_in_microcredits)),
//         ];

//         // Construct the authorization.
//         Authorized::authorize(
//             &private_key,
//             program_id,
//             function_name,
//             inputs,
//             priority_fee_in_microcredits,
//             broadcast,
//             rng,
//         )
//     }

//     /// Returns a transaction that any staker to unbond their microcredits
// from     /// a validator.
//     pub fn unbond_public(
//         private_key: &str,
//         amount_in_microcredits: u64,
//         priority_fee_in_microcredits: u64,
//         broadcast: bool,
//         rng: &mut (impl Rng + CryptoRng),
//     ) -> Result<Authorized> {
//         // Initialize the private key.
//         let private_key = PrivateKey::from_str(private_key)?;
//         // Initialize the amount in microcredits.
//         let amount_in_microcredits = U64::new(amount_in_microcredits);

//         // Construct the program ID and function name.
//         let (program_id, function_name) = ("credits.aleo", "unbond_public");
//         // Construct the inputs.
//         let inputs = vec![Value::from(Literal::U64(amount_in_microcredits))];

//         // Construct the authorization.
//         Authorized::authorize(
//             &private_key,
//             program_id,
//             function_name,
//             inputs,
//             priority_fee_in_microcredits,
//             broadcast,
//             rng,
//         )
//     }

//     /// Returns a transaction that allows a validator to unbond any delegator
//     /// that is bonded to them.
//     pub fn unbond_delegator_as_validator(
//         private_key: &str,
//         delegator: &str,
//         priority_fee_in_microcredits: u64,
//         broadcast: bool,
//         rng: &mut (impl Rng + CryptoRng),
//     ) -> Result<Authorized> {
//         // Initialize the private key.
//         let private_key = PrivateKey::from_str(private_key)?;
//         // Initialize the delegator's address.
//         let delegator = Address::from_str(delegator)?;

//         // Construct the program ID and function name.
//         let (program_id, function_name) = ("credits.aleo",
// "unbond_delegator_as_validator");         // Construct the inputs.
//         let inputs = vec![Value::from(Literal::Address(delegator))];

//         // Construct the authorization.
//         Authorized::authorize(
//             &private_key,
//             program_id,
//             function_name,
//             inputs,
//             priority_fee_in_microcredits,
//             broadcast,
//             rng,
//         )
//     }

//     /// Returns a transaction that allows any staker to claim their
// microcredits     /// after the unbonding period.
//     pub fn claim_unbond_public(
//         private_key: &str,
//         priority_fee_in_microcredits: u64,
//         broadcast: bool,
//         rng: &mut (impl Rng + CryptoRng),
//     ) -> Result<Authorized> {
//         // Initialize the private key.
//         let private_key = PrivateKey::from_str(private_key)?;

//         // Construct the program ID and function name.
//         let (program_id, function_name) = ("credits.aleo",
// "claim_unbond_public");

//         // Construct the authorization.
//         Authorized::authorize(
//             &private_key,
//             program_id,
//             function_name,
//             vec![],
//             priority_fee_in_microcredits,
//             broadcast,
//             rng,
//         )
//     }

//     /// Returns a transaction that allows a validator to set their state to
// be     /// either opened or closed to stakers.
//     pub fn set_validator_state(
//         private_key: &str,
//         is_open: bool,
//         priority_fee_in_microcredits: u64,
//         broadcast: bool,
//         rng: &mut (impl Rng + CryptoRng),
//     ) -> Result<Authorized> {
//         // Initialize the private key.
//         let private_key = PrivateKey::from_str(private_key)?;
//         // Initialize the 'is_open' boolean flag.
//         let is_open = Boolean::new(is_open);

//         // Construct the program ID and function name.
//         let (program_id, function_name) = ("credits.aleo",
// "set_validator_state");         // Construct the inputs.
//         let inputs = vec![Value::from(Literal::Boolean(is_open))];

//         // Construct the authorization.
//         Authorized::authorize(
//             &private_key,
//             program_id,
//             function_name,
//             inputs,
//             priority_fee_in_microcredits,
//             broadcast,
//             rng,
//         )
//     }
//     /// Returns a transaction that transfers public credits from the sender
// to     /// the recipient.
//     pub fn transfer_public(
//         private_key: PrivateKey,
//         recipient: Address,
//         amount_in_microcredits: u64,
//         priority_fee_in_microcredits: u64,
//         broadcast: bool,
//         rng: &mut (impl Rng + CryptoRng),
//     ) -> Result<Authorized> {
//         // Initialize the amount in microcredits.
//         let amount_in_microcredits = U64::new(amount_in_microcredits);

//         // Construct the program ID and function name.
//         let (program_id, function_name) = ("credits.aleo",
// "transfer_public");         // Construct the inputs.
//         let inputs = vec![
//             Value::from(Literal::Address(recipient)),
//             Value::from(Literal::U64(amount_in_microcredits)),
//         ];

//         // Construct the authorization.
//         Authorized::authorize(
//             &private_key,
//             program_id,
//             function_name,
//             inputs,
//             priority_fee_in_microcredits,
//             broadcast,
//             rng,
//         )
//     }

//     /// Returns a transaction that transfers public to private credits from
// the     /// sender to the recipient.
//     pub fn transfer_public_to_private(
//         private_key: &str,
//         recipient: &str,
//         amount_in_microcredits: u64,
//         priority_fee_in_microcredits: u64,
//         broadcast: bool,
//         rng: &mut (impl Rng + CryptoRng),
//     ) -> Result<Authorized> {
//         // Initialize the private key.
//         let private_key = PrivateKey::from_str(private_key)?;
//         // Initialize the recipient.
//         let recipient = Address::from_str(recipient)?;
//         // Initialize the amount in microcredits.
//         let amount_in_microcredits = U64::new(amount_in_microcredits);

//         // Construct the program ID and function name.
//         let (program_id, function_name) = ("credits.aleo",
// "transfer_public_to_private");         // Construct the inputs.
//         let inputs = vec![
//             Value::from(Literal::Address(recipient)),
//             Value::from(Literal::U64(amount_in_microcredits)),
//         ];

//         // Construct the authorization.
//         Authorized::authorize(
//             &private_key,
//             program_id,
//             function_name,
//             inputs,
//             priority_fee_in_microcredits,
//             broadcast,
//             rng,
//         )
//     }
// }
