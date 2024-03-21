use std::str::FromStr;

use anyhow::Result;
use clap::Subcommand;
use snarkvm::{
    circuit::AleoV0,
    console::{
        network::MainnetV0,
        program::{Entry, Identifier, Literal, Plaintext},
    },
    ledger::{
        store::{
            helpers::{MapRead, NestedMapRead},
            BlockStorage, CommitteeStorage, FinalizeStorage,
        },
        RecordsFilter,
    },
    prelude::Zero,
    synthesizer::Program,
};

use crate::{ledger::util, Address, DbLedger, PrivateKey, ViewKey};

#[derive(Debug, Subcommand)]
pub enum View {
    Top,
    Block { block_height: u32 },
    Balance { address: String },
    Records { private_key: PrivateKey },
}

impl View {
    pub fn parse(self, ledger: &DbLedger) -> Result<()> {
        match self {
            View::Block { block_height } => {
                // Print information about the ledger
                println!("{:#?}", ledger.get_block(block_height)?);
            }
            View::Top => {
                println!("{:#?}", ledger.latest_block());
            }
            View::Balance { address } => {
                let addr = Address::from_str(&address)?;

                println!("{address} balance {}", util::get_balance(addr, ledger)?);
            }
            View::Records { private_key } => {
                let view_key = ViewKey::try_from(private_key)?;

                let microcredits = Identifier::from_str("microcredits").unwrap();
                let records = ledger
                    .find_records(&view_key, RecordsFilter::SlowUnspent(private_key))
                    .unwrap()
                    .filter(|(_, record)| match record.data().get(&microcredits) {
                        Some(Entry::Private(Plaintext::Literal(Literal::U64(amount), _))) => {
                            !amount.is_zero()
                        }
                        _ => false,
                    })
                    .collect::<indexmap::IndexMap<_, _>>();

                let address = Address::try_from(private_key)?;

                println!(
                    "{address} records {}",
                    serde_json::to_string_pretty(&records)?
                );
            }
        }
        Ok(())
    }
}
