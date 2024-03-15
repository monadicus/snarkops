use anyhow::Result;

use crate::schema::storage::{AccountSources, Document, LedgerStorage};

pub struct PreparedStorage {
    pub storage: LedgerStorage,
    pub accounts: Option<AccountSources>,
}

impl PreparedStorage {
    pub fn prepare(document: Document) -> Result<PreparedStorage> {
        // the storage document describes some generation details
        if let Some(generate) = document.generate {
            let genesis_output = generate.genesis.output.to_owned();

            // generate the genesis block
            generate.genesis.parse()?;

            // initialize the ledger
            snarkos_aot::ledger::Ledger {
                enable_profiling: false,
                genesis: genesis_output,
                ledger: generate.ledger.output,
                command: snarkos_aot::ledger::Commands::Init(snarkos_aot::ledger::init::Init),
            }
            .parse()?;
        }

        Ok(Self {
            storage: document.storage,
            accounts: document.accounts,
        })
    }
}
