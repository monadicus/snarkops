use super::*;

#[derive(Debug, Args)]
#[group(required = true, multiple = false)]
pub struct Truncate {
    #[arg(long)]
    height: Option<u32>,
    #[arg(long)]
    amount: Option<u32>,
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
