# Runners

A runner is a `snarkos` node runner. Of which we have two different ways.

## Snarkos

The regular `snarkos` installation and runner.

## Snarkos AoT

Our own custom wrapper around `snarkos` that allows for some custom behaviors:

- Additional logging setup/changes
- Custom ports
- Ahead-of-Time(AoT) generation of public transactions
- Custom Genesis creation
- Ledger manipulation and viewing tools
- Ledger checkpoint tooling