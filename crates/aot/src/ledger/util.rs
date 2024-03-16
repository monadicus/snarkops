use std::{fs, path::PathBuf, str::FromStr};

use aleo_std::StorageMode;
use anyhow::{bail, Result};
use indexmap::IndexMap;
use rand::{thread_rng, CryptoRng, Rng, SeedableRng};
use rand_chacha::ChaChaRng;
use snarkvm::{
    circuit::{Aleo, AleoV0},
    console::{
        account::{PrivateKey, ViewKey},
        program::{Ciphertext, Identifier, Literal, Plaintext, ProgramID, Record, Value},
        types::{Address, Field, U64},
    },
    ledger::{query::Query, store::ConsensusStorage, Block, Execution, Fee, Ledger, Transaction},
    prelude::{execution_cost, Network},
    synthesizer::VM,
    utilities::FromBytes,
};
use tracing::{span, Level};

use super::*;

#[tracing::instrument]
pub fn open_ledger<N: Network, C: ConsensusStorage<N>>(
    genesis_path: PathBuf,
    ledger_path: PathBuf,
) -> Result<Ledger<N, C>> {
    let genesis_block = Block::read_le(fs::File::open(genesis_path)?)?;

    Ledger::load(genesis_block, StorageMode::Custom(ledger_path))
}

pub fn prove_credits<N: Network, C: ConsensusStorage<N>, A: Aleo<Network = N>>(
    locator: &'static str,
    vm: &VM<N, C>,
    private_key: PrivateKey<N>,
    inputs: impl IntoIterator<IntoIter = impl ExactSizeIterator<Item = impl TryInto<Value<N>>>>,
) -> Result<Execution<N>> {
    let rng = &mut rand::thread_rng();

    // authorize the transfer execution
    let auth = vm.authorize(
        &private_key,
        ProgramID::from_str("credits.aleo")?,
        Identifier::from_str(locator)?,
        inputs,
        rng,
    )?;

    // assemble the proof
    let (_, mut trace) = vm.process().read().execute::<A, _>(auth, rng)?;
    trace.prepare(Query::from(vm.block_store()).clone())?;
    trace.prove_execution::<A, _>(&format!("credits.aleo/{locator}"), rng)
}

pub fn prove_fee<N: Network, C: ConsensusStorage<N>, A: Aleo<Network = N>>(
    vm: &VM<N, C>,
    private_key: &PrivateKey<N>,
    min_fee: u64,
    execution_id: Field<N>,
) -> Result<Fee<N>> {
    let rng = &mut rand::thread_rng();

    // authorize the fee execution
    let auth = vm.authorize_fee_public(private_key, min_fee, 0, execution_id, rng)?;

    // assemble the proof
    let (_, mut trace) = vm.process().read().execute::<A, _>(auth, rng)?;
    trace.prepare(Query::from(vm.block_store()).clone())?;
    trace.prove_fee::<A, _>(rng)
}

pub fn public_transaction<N: Network, C: ConsensusStorage<N>, A: Aleo<Network = N>>(
    locator: &'static str,
    vm: &VM<N, C>,
    address: Address<N>,
    amount_microcredits: u64,
    private_key: PrivateKey<N>,
    private_key_fee: Option<PrivateKey<N>>,
) -> Result<Transaction<N>> {
    let rng = &mut rand::thread_rng();

    let query = Query::from(vm.block_store());

    // fee key falls back to the private key
    let private_key_fee = private_key_fee.unwrap_or(private_key);

    // proof for the execution of the transfer function
    let execution = prove_credits::<_, _, A>(
        locator,
        vm,
        private_key,
        vec![
            Value::from_str(address.to_string().as_str())?,
            Value::from(Literal::U64(U64::new(amount_microcredits))),
        ],
    )?;

    // compute fee for the execution
    let (min_fee, _) = execution_cost(&vm.process().read(), &execution)?;

    // proof for the fee, authorizing the execution
    let fee = prove_fee::<_, _, A>(vm, &private_key_fee, min_fee, execution.to_execution_id()?)?;

    // assemble the transaction
    Transaction::<N>::from_execution(execution, Some(fee))
}

pub fn make_transaction_proof<N: Network, C: ConsensusStorage<N>, A: Aleo<Network = N>>(
    vm: &VM<N, C>,
    address: Address<N>,
    amount_microcredits: u64,
    private_key: PrivateKey<N>,
    private_key_fee: Option<PrivateKey<N>>,
) -> Result<Transaction<N>> {
    public_transaction::<_, _, A>(
        "transfer_public",
        vm,
        address,
        amount_microcredits,
        private_key,
        private_key_fee,
    )
}

pub fn make_transaction_proof_private<N: Network, C: ConsensusStorage<N>, A: Aleo<Network = N>>(
    vm: &VM<N, C>,
    address: Address<N>,
    amounts: Vec<u64>,
    private_key: PrivateKey<N>,
    private_key_fee: Option<PrivateKey<N>>,
) -> Result<(Transaction<N>, Vec<Transaction<N>>)> {
    let record_tx = public_transaction::<_, _, A>(
        "transfer_public_to_private",
        vm,
        Address::try_from(private_key)?,
        amounts.iter().sum(),
        private_key,
        private_key_fee,
    )?;

    // fee key falls back to the private key
    let private_key_fee = private_key_fee.unwrap_or(private_key);

    // Cannot fail because transfer_public_to_private always emits a record
    let record_enc: Record<N, Ciphertext<N>> = record_tx.records().next().unwrap().1.clone();
    // Decrypt the record
    let record = record_enc.decrypt(&ViewKey::try_from(private_key)?)?;

    let query = Query::from(vm.block_store());

    let mut transactions = Vec::with_capacity(amounts.len());

    for amount in amounts {
        let rng = &mut rand::thread_rng();

        // proof for the execution of the transfer function
        let execution = prove_credits::<_, _, A>(
            "transfer_private",
            vm,
            private_key,
            vec![
                Value::Record(record.clone()),
                Value::from_str(address.to_string().as_str())?,
                Value::from(Literal::U64(U64::new(amount))),
            ],
        )?;

        // compute fee for the execution
        let (min_fee, _) = execution_cost(&vm.process().read(), &execution)?;

        // proof for the fee, authorizing the execution
        let fee =
            prove_fee::<_, _, A>(vm, &private_key_fee, min_fee, execution.to_execution_id()?)?;

        // assemble the transaction
        transactions.push(Transaction::<N>::from_execution(execution, Some(fee))?);
    }

    Ok((record_tx, transactions))
}

pub fn get_balance<N: Network, C: ConsensusStorage<N>>(
    addr: Address<N>,
    ledger: &Ledger<N, C>,
) -> Result<u64> {
    let balance = ledger.vm().finalize_store().get_value_confirmed(
        ProgramID::try_from("credits.aleo")?,
        Identifier::try_from("account")?,
        &Plaintext::from(Literal::Address(addr)),
    )?;

    match balance {
        Some(Value::Plaintext(Plaintext::Literal(Literal::U64(balance), _))) => Ok(*balance),
        None => bail!("No balance found for address: {addr}"),
        _ => unreachable!(),
    }
}

pub fn add_block_with_transactions<N: Network, C: ConsensusStorage<N>, R: Rng + CryptoRng>(
    ledger: &Ledger<N, C>,
    private_key: PrivateKey<N>,
    transactions: Vec<Transaction<N>>,
    rng: &mut R,
) -> Result<Block<N>> {
    let block = ledger.prepare_advance_to_next_beacon_block(
        &private_key,
        vec![],
        vec![],
        transactions,
        rng,
    )?;
    ledger.advance_to_next_block(&block)?;
    Ok(block)
}

pub fn add_transaction_blocks<N: Network, C: ConsensusStorage<N>, R: Rng + CryptoRng>(
    ledger: &Ledger<N, C>,
    private_key: PrivateKey<N>,
    transactions: &[Transaction<N>],
    per_block: usize,
    rng: &mut R,
) -> Result<usize> {
    let mut count = 0;

    for chunk in transactions.chunks(per_block) {
        let target_block = ledger.prepare_advance_to_next_beacon_block(
            &private_key,
            vec![],
            vec![],
            chunk.to_vec(),
            rng,
        )?;

        ledger.advance_to_next_block(&target_block)?;
        count += 1;
    }

    Ok(count)
}

pub fn gen_n_tx<'a, C: ConsensusStorage<crate::Network>>(
    ledger: &'a Ledger<crate::Network, C>,
    private_keys: &'a PrivateKeys,
    num_tx: u64,
    max_tx_credits: Option<u64>,
) -> impl Iterator<Item = Result<Transaction<crate::Network>>> + 'a {
    let tx_span = span!(Level::INFO, "tx generation");
    (0..num_tx).into_iter().map(move |_| {
        let _enter = tx_span.enter();

        let mut rng = ChaChaRng::from_rng(thread_rng())?;

        let keys = private_keys.random_accounts(&mut rng);

        let from = Address::try_from(keys[1])?;
        let amount = match max_tx_credits {
            Some(amount) => rng.gen_range(1..amount),
            None => rng.gen_range(1..get_balance(from, ledger)? / 2),
        };

        let to = Address::try_from(keys[0])?;

        let proof_span = span!(Level::INFO, "tx generation proof");
        let _enter = proof_span.enter();

        make_transaction_proof::<_, _, AleoV0>(
            ledger.vm(),
            to,
            amount,
            keys[1],
            keys.get(2).copied(),
        )
    })
}
