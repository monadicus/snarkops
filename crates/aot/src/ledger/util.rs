// Copyright (C) 2019-2023 Aleo Systems Inc.
// This file is part of the snarkOS library.

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at:
// http://www.apache.org/licenses/LICENSE-2.0

// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#![allow(dead_code)]

use std::{
    fs,
    path::{Path, PathBuf},
    str::FromStr,
};

use aleo_std::StorageMode;
use anyhow::{bail, Result};
use rand::{CryptoRng, Rng};
use serde::Serialize;
use snarkvm::{
    circuit::Aleo,
    console::{
        account::PrivateKey,
        program::{Identifier, Literal, Network, Plaintext, ProgramID, Value},
        types::{Address, U64},
    },
    ledger::{
        query::Query,
        store::{helpers::rocksdb::ConsensusDB, ConsensusStorage},
        Block, Ledger, Transaction,
    },
    synthesizer::{process::execution_cost, VM},
    utilities::FromBytes,
};

pub fn write_json_to<T: Serialize>(to: &Path, t: &T) -> Result<()> {
    let file = fs::File::options()
        .append(false)
        .create(true)
        .write(true)
        .open(to)?;
    serde_json::to_writer_pretty(file, t)?;

    Ok(())
}

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

pub fn get_balance<N: Network>(
    addr: Address<N>,
    ledger: &Ledger<N, ConsensusDB<N>>,
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

pub fn add_block_with_transactions<N: Network, S: ConsensusStorage<N>, R: Rng + CryptoRng>(
    ledger: &Ledger<N, S>,
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

pub fn add_transaction_blocks<N: Network, S: ConsensusStorage<N>, R: Rng + CryptoRng>(
    ledger: &Ledger<N, S>,
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
