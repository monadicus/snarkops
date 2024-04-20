use anyhow::Result;
use clap::Parser;
use reqwest::blocking::{Client, Response};

/// For listing different resources.
#[derive(Debug, Parser)]
pub struct List {
    #[clap(subcommand)]
    command: Commands,
}

/// List commands
#[derive(Debug, Parser)]
enum Commands {
    /// List all environments.
    Env,
}

impl List {
    pub fn run(self, url: &str, client: Client) -> Result<Response> {
        use Commands::*;
        Ok(match self.command {
            Env => {
                let ep = format!("{url}/api/v1/env/list");

                client.get(ep).send()?
            }
        })
    }
}
