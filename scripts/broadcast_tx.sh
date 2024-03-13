#!/usr/bin/env bash

# broadcasts a single transaction from the tx.json file

TEST_PATH=$(scripts/test_path.sh)
TRANSACTIONS=$TEST_PATH/tx.json

# broadcast a transaction by index from the tx.json file
curl -H "Content-Type: application/json" -d "$(cat $TRANSACTIONS | jq ".[$1]")" \
  http://localhost:3030/mainnet/transaction/broadcast
