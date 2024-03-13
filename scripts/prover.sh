#!/usr/bin/env bash

# spin up a prover using committee member <arg 1>'s key

if [ -z "$1" ]; then
  echo "Usage: $0 <node_id>"
  exit 1
fi

TEST_PATH=$(scripts/test_path.sh)
SNARKOS="$(pwd)/../snarkos/target/release/snarkos"

GENESIS=$TEST_PATH/genesis.block
COMMITTEE=$TEST_PATH/committee.json
LEDGER=$TEST_PATH/ledger

pk() { cat $COMMITTEE | jq "[.[][0]][$1]" -r; }

cp -r $LEDGER $LEDGER"_prover_$1"

$SNARKOS start --nodisplay --prover --nocdn \
  --rest-rps 1000 \
  --verbosity 4 \
  --bft "0.0.0.0:500$1" \
  --rest "0.0.0.0:303$1" \
  --genesis $GENESIS \
  --storage_path $LEDGER"_prover_$1" \
  --private-key $(pk $1) \
  --trusted-peers "127.0.0.1:4130,127.0.0.1:4131,127.0.0.1:4132,127.0.0.1:4133" \
  --validators "127.0.0.1:5000" \
  --node "0.0.0.0:413$1"
