#!/usr/bin/env bash

# generate 2 transactions and add them to the ledger

BINARY="$(pwd)/target/release/snarkos-aot"

TEST_PATH=$(scripts/test_path.sh)
GENESIS=$TEST_PATH/genesis.block
LEDGER=$TEST_PATH/ledger

$BINARY ledger -g $GENESIS -l $LEDGER view block 0
