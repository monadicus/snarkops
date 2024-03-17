use anyhow::{ensure, Result};
use clap::Args;
use rand::{CryptoRng, Rng};
use snarkvm::ledger::Transaction;

use crate::{ledger::util, DbLedger, Network, PrivateKey, VM};

#[derive(Debug, Args)]
pub struct Stdin {
    /// The private key to use when generating the block.
    #[arg(name = "private-key", long)]
    private_key: Option<PrivateKey>,
    /// The number of transactions to add per block.
    #[arg(name = "txs-per-block", long)]
    txs_per_block: Option<usize>,
}

impl Stdin {
    pub fn parse<R: Rng + CryptoRng>(self, ledger: &DbLedger, rng: &mut R) -> Result<()> {
        // Ensure we aren't trying to stick too many transactions into a block
        let per_block_max = VM::MAXIMUM_CONFIRMED_TRANSACTIONS;
        let per_block = self.txs_per_block.unwrap_or(per_block_max);
        ensure!(
            per_block <= per_block_max,
            "too many transactions per block (max is {})",
            per_block_max
        );

        // Get the block private key
        let private_key = match self.private_key {
            Some(pk) => pk,
            None => PrivateKey::new(rng)?,
        };

        // Stdin line buffer
        let mut buf = String::new();

        // Transaction buffer
        // TODO: convert this into a TxCannon type and ensure all Dependent transactions are added in separate blocks
        let mut tx_buf: Vec<Transaction<Network>> = Vec::with_capacity(per_block);

        // Macro to commit a block into the buffer
        // This can't trivially be a closure because of... you guessed it... the borrow
        // checker
        let mut num_blocks = 0;
        macro_rules! commit_block {
            () => {
                let buf_size = tx_buf.len();
                let block = util::add_block_with_transactions(
                    &ledger,
                    private_key,
                    std::mem::replace(&mut tx_buf, Vec::with_capacity(per_block)),
                    rng,
                )?;

                println!(
                    "Inserted a block with {buf_size} transactions to the ledger (hash: {})",
                    block.hash()
                );
                num_blocks += 1;
            };
        }

        loop {
            // Clear the buffer
            buf.clear();

            // Read a line, and match on how many characters we read
            match std::io::stdin().read_line(&mut buf)? {
                // We've reached EOF
                0 => {
                    if !tx_buf.is_empty() {
                        commit_block!();
                    }
                    break;
                }

                // Not at EOF
                _ => {
                    // Remove newline from buffer
                    buf.pop();

                    // Skip if buffer is now empty
                    if buf.is_empty() {
                        continue;
                    }

                    // Deserialize the transaction
                    let Ok(tx) = serde_json::from_str(&buf) else {
                        eprintln!("Failed to deserialize transaction: {buf}");
                        continue;
                    };

                    // Commit if the buffer is now big enough
                    tx_buf.push(tx);
                    if tx_buf.len() >= per_block {
                        commit_block!();
                    }
                }
            }
        }

        println!("Inserted {num_blocks} blocks into the ledger");
        Ok(())
    }
}
