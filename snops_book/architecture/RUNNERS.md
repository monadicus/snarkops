# Runners

A runner is a `snarkos` node runner.

Of which we currently only support our own wrapper around `snarkos`.

## Snarkos AoT

Our own custom wrapper around `snarkos` that allows for some custom behaviors:

- configurable BFT port
- configurable genesis block
- configurable ledger paths
- realtime ledger checkpoints
- loki integration
- ahead-of-time public transaction generation
- transaction executions
