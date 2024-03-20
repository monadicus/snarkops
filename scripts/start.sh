#!/usr/bin/env bash

# spin up a client using committee member 0's key

MODE="$1"
INDEX="$2"

# check if mode is client, validator, or prover
if [ -z "$MODE" ] || [ "$MODE" != "client" ] && [ "$MODE" != "validator" ] && [ "$MODE" != "prover" ]; then
  echo "Usage: $0 <client|validator|prover> <node_id>"
  exit 1
fi

# check if index is set
if [ -z "$INDEX" ]; then
  echo "Usage: $0 <client|validator|prover> <node_id>"
  exit 1
fi

TEST_PATH=$(scripts/test_path.sh)
BINARY="$(pwd)/target/release/snarkos-aot"

GENESIS=$TEST_PATH/genesis.block
COMMITTEE=$TEST_PATH/committee.json
LEDGER=$TEST_PATH/ledger

pk() { cat $COMMITTEE | jq "[.[][0]][$INDEX]" -r; }

STORAGE="${LEDGER}_${MODE}_${INDEX}"

# delete the old ledger
# rm -rf $STORAGE
# cp -r $LEDGER $STORAGE


$BINARY run --type $MODE \
  --bft "500$INDEX" \
  --rest "303$INDEX" \
  --node "413$INDEX" \
  --genesis $GENESIS \
  --ledger $STORAGE \
  --log "${TEST_PATH}/${MODE}_${INDEX}.log" \
  --private-key $(pk 0) \
  --peers "127.0.0.1:4130,127.0.0.1:4131,127.0.0.1:4132,127.0.0.1:4133,127.0.0.1:4134" \
  --validators "127.0.0.1:5000,127.0.0.1:5001,127.0.0.1:5002,127.0.0.1:5003,127.0.0.1:5004,127.0.0.1:5005"
