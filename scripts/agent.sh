#!/usr/bin/env bash

# spin up an agent with unique ports

INDEX="$1"
shift

# check if index is set
if [ -z "$INDEX" ]; then
  echo "Usage: $0 <node_id>"
  exit 1
fi

ENDPOINT="127.0.0.1:1234"
DATA_PATH="$(pwd)/snops-data/$INDEX"
AGENT_BIN="$DATA_PATH/agent"

# create the data path if it doesn't exist
mkdir -p "$DATA_PATH"

echo "Starting agent in ${DATA_PATH}"

# Download the agent binary
echo "Checking for agent binary..."

# conditionally set the -z flag to check if the file exists
if [ -e "$AGENT_BIN" ]
then zflag="-z '$AGENT_BIN'"
else zflag=
fi

curl -sSL "$ENDPOINT/content/agent" $zflag -o $AGENT_BIN
chmod +x $AGENT_BIN

$AGENT_BIN \
  --endpoint "$ENDPOINT" \
  --id "local-$INDEX" \
  --path "$DATA_PATH" \
  --bind "0.0.0.0" \
  --bft "$((5000 + $INDEX))" \
  --rest "$((3030 + $INDEX))" \
  --metrics "$((9000 + $INDEX))" \
  --node "$((4130 + $INDEX))" \
  --labels "local,local-$INDEX" \
  --client --validator --prover --compute \
  $@

