use anyhow::Result;
use clap::Parser;

mod test;

#[derive(Debug, Parser)]
pub enum Commands {
    Test(test::Test),
}

impl Commands {
    pub fn run(self) -> Result<()> {
        match self {
            Commands::Test(test) => test.run(),
        }
    }
}
