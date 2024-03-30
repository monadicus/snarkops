use std::{os::fd::AsRawFd, path::PathBuf};

use anyhow::Result;
use clap::Args;
use nix::{
    sys::wait::waitpid,
    unistd::{self, ForkResult},
};
use snarkvm::{
    ledger::Block,
    utilities::{FromBytes, ToBytes},
};

use crate::{ledger::util, DbLedger};

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
    pub fn parse(self, genesis: PathBuf, mut ledger: PathBuf) -> Result<()> {
        let (read_fd, write_fd) = unistd::pipe()?;

        match unsafe { unistd::fork() }? {
            ForkResult::Parent { child, .. } => {
                unistd::close(read_fd.as_raw_fd())?;

                let db_ledger: DbLedger = util::open_ledger(genesis, ledger)?;

                let amount = match (self.height, self.amount) {
                    (Some(height), None) => db_ledger.latest_height() - height,
                    (None, Some(amount)) => amount,
                    // Clap should prevent this case
                    _ => unreachable!(),
                };

                let target_height = db_ledger.latest_height().saturating_sub(amount);

                for i in 1..target_height {
                    let block = db_ledger.get_block(i)?;
                    let buf = block.to_bytes_le()?;
                    // println!("Writing block {i}... {}", buf.len());

                    unistd::write(&write_fd, &(buf.len() as u32).to_le_bytes())?;
                    unistd::write(&write_fd, &buf)?;
                }

                unistd::write(&write_fd, &(0u32).to_le_bytes())?;
                unistd::close(write_fd.as_raw_fd())?;

                waitpid(child, None).unwrap();
            }
            ForkResult::Child => {
                unistd::close(write_fd.as_raw_fd())?;

                ledger.set_extension("new");
                let db_ledger: DbLedger = util::open_ledger(genesis, ledger)?;
                let read_fd = read_fd.as_raw_fd();

                loop {
                    let mut size_buf = [0u8; 4];
                    unistd::read(read_fd, &mut size_buf)?;
                    let amount = u32::from_le_bytes(size_buf);
                    if amount == 0 {
                        break;
                    }

                    let mut buf = vec![0u8; amount as usize];
                    let mut read = 0;
                    while read < amount as usize {
                        read += unistd::read(read_fd, &mut buf[read..])?;
                    }
                    let block = Block::from_bytes_le(&buf)?;
                    if db_ledger.latest_height() + 1 != block.height() {
                        println!(
                            "Skipping block {}, waiting for {}",
                            block.height(),
                            db_ledger.latest_height() + 1,
                        );
                    } else {
                        println!(
                            "Reading block {}... {}",
                            db_ledger.latest_height() + 1,
                            buf.len()
                        );

                        db_ledger.advance_to_next_block(&block)?;
                    }
                }

                unistd::close(read_fd.as_raw_fd())?;
                unsafe {
                    nix::libc::_exit(0);
                }
            }
        }

        Ok(())
    }
}
