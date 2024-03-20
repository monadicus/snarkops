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

        ledger.vm().block_store().abort_atomic();
        ledger.vm().finalize_store().abort_atomic();
        dbg!(
            ledger
                .vm()
                .finalize_store()
                .committee_store()
                .current_round(),
            ledger
                .vm()
                .finalize_store()
                .committee_store()
                .current_height(),
            ledger
                .vm()
                .block_store()
                .heights()
                .collect::<Vec<_>>()
                .pop()
        );

        let current_height = dbg!(ledger.latest_height());
        let target_height = dbg!(current_height.saturating_sub(amount) + 1);

        // wipe out N blocks/
        ledger.vm().block_store().remove_last_n(amount)?;

        // remove committee store rounds
        (target_height..=current_height)
            .rev()
            .try_for_each(|height| {
                ledger
                    .vm()
                    .finalize_store()
                    .committee_store()
                    .remove(dbg!(height))
            })?;

        dbg!(
            ledger
                .vm()
                .finalize_store()
                .committee_store()
                .current_round(),
            ledger
                .vm()
                .finalize_store()
                .committee_store()
                .current_height(),
            ledger
                .vm()
                .block_store()
                .heights()
                .collect::<Vec<_>>()
                .pop()
        );

        println!(
            "Removed {amount} blocks from the ledger (new height is {}).",
            ledger
                .latest_height()
                .checked_sub(amount)
                .unwrap_or_default()
        );

        Ok(())
    }
}
