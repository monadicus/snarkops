#!/usr/bin/env bash

# generate 2 transactions and broadcast them to the local client at :3030

BINARY="$(pwd)/target/release/snarkos-aot"

TEST_PATH=$(scripts/test_path.sh)
GENESIS=$TEST_PATH/genesis.block
COMMITTEE=$TEST_PATH/committee.json
ACCOUNTS=$TEST_PATH/accounts.json
TRANSACTIONS=$TEST_PATH/tx.json
LEDGER=$TEST_PATH/ledger

pk() { cat $COMMITTEE | jq "[.[][0]][$1]" -r; }
addr() { cat $COMMITTEE | jq "(. | keys)[$1]" -r; }


# $BINARY ledger -g $GENESIS -l $LEDGER tx num 100 \
#   --private-keys $(cat $ACCOUNTS | jq -r '[to_entries[] | .value[0]] | join(",")') >> $TRANSACTIONS

# emit 3 transactions
# curl -H "Content-Type: application/json" -d "$(cat $TRANSACTIONS | jq ".[0]")" \
#   http://localhost:3030/mainnet/transaction/broadcast
# curl -H "Content-Type: application/json" -d "$(cat $TRANSACTIONS | jq ".[1]")" \
#   http://localhost:3030/mainnet/transaction/broadcast


TX=$($BINARY ledger -g $GENESIS -l $LEDGER tx num 1 --private-keys $(cat $ACCOUNTS | jq -r '[to_entries[] | .value[0]] | join(",")'))
curl -H "Content-Type: application/json" http://localhost:3031/mainnet/transaction/broadcast -d "$TX"
