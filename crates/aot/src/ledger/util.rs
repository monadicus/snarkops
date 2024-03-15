use std::{fs, path::PathBuf, str::FromStr};

use aleo_std::StorageMode;
use anyhow::{bail, Result};
use rand::{thread_rng, CryptoRng, Rng, SeedableRng};
use rand_chacha::ChaChaRng;
use snarkvm::{
    circuit::{Aleo, AleoV0},
    console::{
        account::PrivateKey,
        program::{Identifier, Literal, Plaintext, ProgramID, Value},
        types::{Address, U64},
    },
    ledger::{query::Query, store::ConsensusStorage, Block, Ledger, Transaction},
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

pub fn make_transaction_proof<N: Network, C: ConsensusStorage<N>, A: Aleo<Network = N>>(
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
    let execution = {
        // authorize the transfer execution
        let authorization = vm.authorize(
            &private_key,
            ProgramID::from_str("credits.aleo")?,
            Identifier::from_str("transfer_public")?,
            vec![
                Value::from_str(address.to_string().as_str())?,
                Value::from(Literal::U64(U64::new(amount_microcredits))),
            ]
            .into_iter(),
            rng,
        )?;

        // assemble the proof
        let (_, mut trace) = vm.process().read().execute::<A, _>(authorization, rng)?;
        trace.prepare(query.clone())?;
        trace.prove_execution::<A, _>("credits.aleo/transfer_public", rng)?
    };

    // compute fee for the execution
    let (min_fee, _) = execution_cost(&vm.process().read(), &execution)?;

    // proof for the fee, authorizing the execution
    let fee = {
        // authorize the fee execution
        let fee_authorization =
        // This can have a separate private key because the fee is checked to be VALID
        // and has the associated execution id.
            vm.authorize_fee_public(&private_key_fee, min_fee, 0, execution.to_execution_id()?, rng)?;

        // assemble the proof
        let (_, mut trace) = vm
            .process()
            .read()
            .execute::<A, _>(fee_authorization, rng)?;
        trace.prepare(query)?;
        trace.prove_fee::<A, _>(rng)?
    };

    // assemble the transaction
    Transaction::<N>::from_execution(execution, Some(fee))
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
