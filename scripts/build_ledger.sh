#!/usr/bin/env bash

# generate a genesis block and initial ledger

NAME="$(date +%Y%m%d_%H%M%S)"
BINARY="$(pwd)/target/release/snarkos-aot"

mkdir -p tests/$NAME

GENESIS=tests/$NAME/genesis.block
COMMITTEE=tests/$NAME/committee.json
ACCOUNTS=tests/$NAME/accounts.json
TRANSACTIONS=tests/$NAME/tx.json
LEDGER=tests/$NAME/ledger

echo "Creating test $NAME"

pk() { cat $COMMITTEE | jq "[.[][0]][$1]" -r; }
addr() { cat $COMMITTEE | jq "(. | keys)[$1]" -r; }

# generate the genesis block
$BINARY genesis \
  --committee-size 6 \
  --committee-output $COMMITTEE \
  --output $GENESIS \
  --bonded-balance 10000000000000 \
  --additional-accounts 5 \
  --additional-accounts-output $ACCOUNTS
GENESIS_PK=$(pk 0)

# setup the ledger
$BINARY ledger --genesis $GENESIS --ledger $LEDGER init

echo "Start a validator with \`scripts/validator.sh 0\`"
echo "Broadcast some transactions with \`scripts/tx_cannon.sh\`"
