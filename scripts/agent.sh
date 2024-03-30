#!/usr/bin/env bash

# spin up an agent with unique ports

INDEX="$1"
shift

# check if index is set
if [ -z "$INDEX" ]; then
  echo "Usage: $0 <node_id>"
  exit 1
fi

DATA_PATH="$(pwd)/snot-data/$INDEX"

echo "Starting ${DATA_PATH}"
cargo run --release -p snot-agent -- \
  --id "local-$INDEX" \
  --path "$DATA_PATH" \
  --bind "0.0.0.0" \
  --bft "500$INDEX" \
  --rest "303$INDEX" \
  --metrics "900$INDEX" \
  --node "413$INDEX" \
  --labels "local" \
  --client --validator --compute \
  $@
