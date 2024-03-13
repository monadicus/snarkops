#!/usr/bin/env bash

# generate 2 transactions and broadcast them to the local client at :3030

BINARY="$(pwd)/target/release/snarkos-aot"

TEST_PATH=$(scripts/test_path.sh)
GENESIS=$TEST_PATH/genesis.block
COMMITTEE=$TEST_PATH/committee.json
TRANSACTIONS=$TEST_PATH/tx.json
LEDGER=$TEST_PATH/ledger

pk() { cat $COMMITTEE | jq "[.[][0]][$1]" -r; }
addr() { cat $COMMITTEE | jq "(. | keys)[$1]" -r; }

OPERATIONS=$(jq -r -n \
  --arg genesis_pk $(pk 0) \
  --arg addr_1 $(addr 1) \
  --arg addr_2 $(addr 2) \
'[
  { "from": $genesis_pk, "to": $addr_1, "amount": 500 },
  { "from": $genesis_pk, "to": $addr_2, "amount": 500 }
]')


$BINARY ledger tx -g $GENESIS -l $LEDGER --operations "$OPERATIONS" --output $TRANSACTIONS

# emit 3 transactions
curl -H "Content-Type: application/json" -d "$(cat $TRANSACTIONS | jq ".[0]")" \
  http://localhost:3030/mainnet/transaction/broadcast
curl -H "Content-Type: application/json" -d "$(cat $TRANSACTIONS | jq ".[1]")" \
  http://localhost:3030/mainnet/transaction/broadcast

