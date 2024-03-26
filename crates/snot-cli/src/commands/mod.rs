use anyhow::Result;
use clap::Parser;

mod env;

#[derive(Debug, Parser)]
pub enum Commands {
    Env(env::Env),
}

impl Commands {
    pub fn run(self) -> Result<()> {
        match self {
            Commands::Env(test) => test.run(),
        }
    }
}
