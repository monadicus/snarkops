use anyhow::Result;
use clap::Args;

use crate::DbLedger;

#[derive(Debug, Args)]
#[group(required = true, multiple = false)]
pub struct Truncate {
    #[arg(long)]
    height: Option<u32>,
    #[arg(long)]
    amount: Option<u32>,
    // TODO: duration based truncation (blocks within a duration before now)
    // TODO: timestamp based truncation (blocks after a certain date)
}

impl Truncate {
    pub fn parse(self, ledger: &DbLedger) -> Result<()> {
        let amount = match (self.height, self.amount) {
            (Some(height), None) => ledger.latest_height() - height,
            (None, Some(amount)) => amount,

            // Clap should prevent this case
            _ => unreachable!(),
        };

        ledger.vm().block_store().remove_last_n(amount)?;

        // TODO: is latest_height accurate here?
        println!(
            "Removed {amount} blocks from the ledger (new height is {}).",
            ledger.latest_height()
        );

        Ok(())
    }
}
