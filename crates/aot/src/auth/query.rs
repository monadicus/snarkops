use std::collections::{HashMap, VecDeque};

use anyhow::{anyhow, Result};
use snarkvm::{
    prelude::ProgramID,
    synthesizer::{Authorization, Process, Program, Stack},
};

use crate::Network;

/// Fetches a program from the query endpoint.
pub fn fetch_program<N: Network>(id: ProgramID<N>, query: &str) -> Result<Program<N>> {
    Ok(reqwest::blocking::get(format!("{query}/{}/program/{id}", N::str_id()))?.json()?)
}

/// Walks the program's imports and fetches them all.
pub fn load_program<N: Network>(
    process: &mut Process<N>,
    program_id: ProgramID<N>,
    query: &str,
) -> Result<()> {
    let program = fetch_program(program_id, query)?;

    if process.contains_program(&program_id) {
        return Ok(());
    }

    for import_id in program.imports().keys() {
        if !process.contains_program(import_id) {
            load_program(process, program_id, query)?;
        }
    }

    if !process.contains_program(program.id()) {
        process.add_program(&program)?;
    }

    Ok(())
}

/// Walks the program's imports and fetches them all.
pub fn get_imports<N: Network>(
    process: &Process<N>,
    program: &Program<N>,
    query: &str,
) -> Result<HashMap<ProgramID<N>, Program<N>>> {
    let mut imported = HashMap::new();
    let mut queue = VecDeque::new();
    queue.push_back(program.clone());

    while let Some(program) = queue.pop_front() {
        for import in program.imports().keys() {
            // ignore walking through...
            // ...programs already in the process
            // ...programs already visited
            // ...credits.aleo (potentially redundant case)
            if process.contains_program(import)
                || imported.contains_key(import)
                || *import == N::credits()
            {
                continue;
            }

            let import_program = fetch_program(*import, query)
                .map_err(|e| anyhow!("failed to fetch imported program {import}: {e:?}"))?;

            imported.insert(*import, import_program.clone());
            queue.push_back(import_program);
        }
    }

    Ok(imported)
}

pub fn get_process_imports<N: Network>(
    process: &mut Process<N>,
    program: &Program<N>,
    query: Option<&str>,
) -> Result<()> {
    let imports = query
        .map(|query| get_imports(process, program, query))
        .transpose()?
        .unwrap_or_default();

    for (_, import) in imports {
        process.add_stack(Stack::new(process, &import)?);
    }

    Ok(())
}

pub fn add_program_to_process<N: Network>(
    process: &mut Process<N>,
    program_id: ProgramID<N>,
    query: &str,
) -> Result<()> {
    // if the process already contains the program, we're done
    if process.contains_program(&program_id) {
        return Ok(());
    }

    let program = fetch_program(program_id, query)
        .map_err(|e| anyhow!("failed to fetch program {program_id}: {e:?}"))?;

    get_process_imports(process, &program, Some(query))?;
    process.add_program(&program)?;

    Ok(())
}

pub fn add_many_programs_to_process<N: Network>(
    process: &mut Process<N>,
    programs: Vec<ProgramID<N>>,
    query: &str,
) -> Result<()> {
    for program in programs {
        add_program_to_process(process, program, query)?;
    }

    Ok(())
}

pub fn get_programs_from_auth<N: Network>(auth: &Authorization<N>) -> Vec<ProgramID<N>> {
    auth.transitions()
        .values()
        .filter_map(|req| {
            let id = *req.program_id();
            (id != N::credits()).then_some(id)
        })
        .collect()
}
