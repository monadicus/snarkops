use super::*;

mod random;
mod stdin;

#[derive(Debug, Subcommand)]
pub enum Add {
    Random(random::Random),
    Stdin(stdin::Stdin),
}

impl Add {
    pub fn parse(self, ledger: &DbLedger) -> Result<()> {
        let mut rng = ChaChaRng::from_entropy();
        match self {
            Add::Random(random) => random.parse(ledger, &mut rng),
            Add::Stdin(stdin) => stdin.parse(ledger, &mut rng),
        }
    }
}
