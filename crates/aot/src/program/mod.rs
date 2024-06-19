use anyhow::Result;
use clap::{Args, Subcommand};
use clap_stdin::FileOrStdin;
use serde_json::json;
use snarkvm::synthesizer::Program;

use crate::Network;
pub mod cost;

/// A command to help gather information about a program, including its cost and
/// imports.
#[derive(Debug, Subcommand)]
pub enum ProgramCommand<N: Network> {
    /// Get the ID of a given program.
    Id(ProgramInfo<N>),
    /// List the functions and their inputs/outputs of a given program.
    #[clap(alias = "fn")]
    Functions(ProgramInfo<N>),
    /// List the inputs of a given program.
    Imports(ProgramInfo<N>),
    Cost(cost::CostCommand<N>),
}

impl<N: Network> ProgramCommand<N> {
    pub fn parse(self) -> Result<()> {
        match self {
            ProgramCommand::Id(ProgramInfo { program, json }) => {
                if json {
                    println!("{}", serde_json::to_string(&program.contents()?.id())?);
                } else {
                    println!("{}", program.contents()?.id());
                }
                Ok(())
            }
            ProgramCommand::Functions(ProgramInfo { program, json }) => {
                let program = program.contents()?;
                if json {
                    let mut functions = indexmap::IndexMap::new();
                    for (id, function) in program.functions() {
                        functions.insert(
                            id,
                            json!({
                                "inputs": function.input_types(),
                                "outputs": function.output_types(),
                            }),
                        );
                    }
                    println!("{}", serde_json::to_string(&functions)?);
                } else {
                    for (id, function) in program.functions() {
                        println!("name: {id}");
                        println!("inputs:");
                        for input in function.input_types() {
                            println!("  {input}");
                        }
                        println!("outputs:");
                        for output in &function.output_types() {
                            println!("  {output}");
                        }
                        println!();
                    }
                }
                Ok(())
            }
            ProgramCommand::Imports(ProgramInfo { program, json }) => {
                let program = program.contents()?;
                if json {
                    println!(
                        "{}",
                        serde_json::to_string(&program.imports().keys().collect::<Vec<_>>())?
                    );
                } else {
                    for (id, _import) in program.imports() {
                        println!("{id}");
                    }
                }
                Ok(())
            }
            ProgramCommand::Cost(command) => {
                println!("{}", command.parse()?);
                Ok(())
            }
        }
    }
}

#[derive(Debug, Args)]
pub struct ProgramInfo<N: Network> {
    /// Path to .aleo program to get information about, or `-` for stdin.
    pub program: FileOrStdin<Program<N>>,
    /// Output as JSON
    #[clap(long, short)]
    pub json: bool,
}
