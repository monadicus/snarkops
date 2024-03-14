#!/usr/bin/env bash

# generate 2 transactions and add them to the ledger

BINARY="$(pwd)/target/release/snarkos-aot"

TEST_PATH=$(scripts/test_path.sh)
GENESIS=$TEST_PATH/genesis.block
COMMITTEE=$TEST_PATH/committee.json
TRANSACTIONS=$TEST_PATH/tx.json
LEDGER=$TEST_PATH/ledger

TXS_PER_BLOCK=10

pk() { cat $COMMITTEE | jq "[.[][0]][$1]" -r; }
addr() { cat $COMMITTEE | jq "(. | keys)[$1]" -r; }

OPERATIONS=$(jq -r -n \
  --arg genesis_pk $(pk 0) \
  --arg addr_1 $(addr 1) \
  --arg addr_2 $(addr 2) \
'[
  { "from": $genesis_pk, "to": $addr_1, "amount": 500 },
  { "from": $genesis_pk, "to": $addr_2, "amount": 500 },
  { "from": $genesis_pk, "to": $addr_1, "amount": 500 },
  { "from": $genesis_pk, "to": $addr_2, "amount": 500 },
  { "from": $genesis_pk, "to": $addr_1, "amount": 500 },
  { "from": $genesis_pk, "to": $addr_2, "amount": 500 },
  { "from": $genesis_pk, "to": $addr_1, "amount": 500 },
  { "from": $genesis_pk, "to": $addr_2, "amount": 500 },
  { "from": $genesis_pk, "to": $addr_1, "amount": 500 },
  { "from": $genesis_pk, "to": $addr_2, "amount": 500 },
  { "from": $genesis_pk, "to": $addr_1, "amount": 500 },
  { "from": $genesis_pk, "to": $addr_2, "amount": 500 },
  { "from": $genesis_pk, "to": $addr_1, "amount": 500 },
  { "from": $genesis_pk, "to": $addr_2, "amount": 500 },
  { "from": $genesis_pk, "to": $addr_1, "amount": 500 },
  { "from": $genesis_pk, "to": $addr_2, "amount": 500 },
  { "from": $genesis_pk, "to": $addr_1, "amount": 500 },
  { "from": $genesis_pk, "to": $addr_2, "amount": 500 },
  { "from": $genesis_pk, "to": $addr_1, "amount": 500 },
  { "from": $genesis_pk, "to": $addr_2, "amount": 500 },
  { "from": $genesis_pk, "to": $addr_1, "amount": 500 },
  { "from": $genesis_pk, "to": $addr_2, "amount": 500 },
  { "from": $genesis_pk, "to": $addr_1, "amount": 500 },
  { "from": $genesis_pk, "to": $addr_2, "amount": 500 }
]')

$BINARY ledger -g $GENESIS -l $LEDGER tx --operations "$OPERATIONS" \
  | $BINARY ledger -g $GENESIS -l $LEDGER add --txs-per-block $TXS_PER_BLOCK
