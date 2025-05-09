use aleo_std::StorageMode;
use anyhow::bail;
use rand::{SeedableRng, thread_rng};
use rand_chacha::ChaChaRng;
use snarkvm::{
    algorithms::snark::varuna::VarunaVersion,
    circuit::Aleo,
    console::{
        account::{PrivateKey, ViewKey},
        program::{Ciphertext, Identifier, Literal, Plaintext, ProgramID, Record, Value},
        types::{Address, Field, U64},
    },
    ledger::{Block, Execution, Fee, Ledger, Transaction, query::Query, store::ConsensusStorage},
    prelude::{Network, execution_cost_v2},
    synthesizer::VM,
};

use super::*;

pub fn open_ledger<N: Network, C: ConsensusStorage<N>>(
    genesis_block: Block<N>,
    ledger_path: PathBuf,
) -> Result<Ledger<N, C>> {
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
    trace.prove_execution::<A, _>(&format!("credits.aleo/{locator}"), VarunaVersion::V1, rng)
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
    trace.prove_fee::<A, _>(VarunaVersion::V1, rng)
}

pub fn public_transaction<N: Network, C: ConsensusStorage<N>, A: Aleo<Network = N>>(
    locator: &'static str,
    vm: &VM<N, C>,
    address: Address<N>,
    amount_microcredits: u64,
    private_key: PrivateKey<N>,
    private_key_fee: Option<PrivateKey<N>>,
) -> Result<Transaction<N>> {
    // let rng = &mut rand::thread_rng();

    // let query = Query::from(vm.block_store());

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
    let (min_fee, _) = execution_cost_v2(&vm.process().read(), &execution)?;

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

pub fn _make_transaction_proof_private<N: Network, C: ConsensusStorage<N>, A: Aleo<Network = N>>(
    ledger: &Ledger<N, C>,
    address: Address<N>,
    amounts: Vec<u64>,
    private_key: PrivateKey<N>,
    private_key_fee: Option<PrivateKey<N>>,
) -> Result<(Transaction<N>, Vec<Transaction<N>>)> {
    let vm = ledger.vm();

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

    let mut rng = ChaChaRng::from_rng(thread_rng())?;

    let target_block = ledger.prepare_advance_to_next_beacon_block(
        &private_key,
        vec![],
        vec![],
        vec![record_tx.clone()],
        &mut rng,
    )?;

    ledger.advance_to_next_block(&target_block)?;

    let transactions = amounts
        .into_iter()
        .map(|amount| {
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
            let (min_fee, _) = execution_cost_v2(&vm.process().read(), &execution)?;

            // proof for the fee, authorizing the execution
            let fee =
                prove_fee::<_, _, A>(vm, &private_key_fee, min_fee, execution.to_execution_id()?)?;

            // assemble the transaction
            Transaction::<N>::from_execution(execution, Some(fee))
        })
        .collect::<Result<Vec<_>>>();

    Ok((record_tx, transactions?))
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
