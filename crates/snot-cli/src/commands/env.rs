use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use serde_json::Value;

/// For interacting with snot tests.
#[derive(Debug, Parser)]
pub struct Env {
    /// The url the control plane is on.
    #[clap(short, long, default_value = "http://localhost:1234")]
    url: String,
    #[clap(subcommand)]
    command: Commands,
}

/// Env commands
#[derive(Debug, Parser)]
enum Commands {
    /// Prepare a (test) environment.
    Prepare {
        /// The test spec file.
        spec: PathBuf,
    },

    /// Start an environment's timeline (a test).
    Start { id: usize },

    /// Stop an environment's timeline.
    Stop {
        /// Stop all envs.
        // #[clap(short, long)]
        // all: bool,
        /// Stop a specific test.
        id: usize,
    },
}

impl Env {
    pub fn run(self) -> Result<()> {
        let client = reqwest::blocking::Client::new();

        use Commands::*;
        match self.command {
            Prepare { spec } => {
                let ep = format!("{}/api/v1/env/prepare", self.url);
                let file: String = std::fs::read_to_string(spec)?;

                let id: Value = client.post(ep).body(file).send()?.json()?;
                println!("{}", serde_json::to_string(&id)?);
                Ok(())
            }

            Start { id } => {
                let ep = format!("{}/api/v1/env/{id}", self.url);

                client.post(ep).send()?;
                Ok(())
            }

            Stop { id } => {
                let ep = format!("{}/api/v1/env/{id}", self.url);

                client.delete(ep).send()?;
                Ok(())
            }
        }
    }
}
