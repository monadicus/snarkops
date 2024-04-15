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

echo "Starting agent in ${DATA_PATH}"

while true; do

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
    --bft "500$INDEX" \
    --rest "303$INDEX" \
    --metrics "900$INDEX" \
    --node "413$INDEX" \
    --labels "local,local-$INDEX" \
    --client --validator --compute \
    $@

  echo "Agent closed.. Rebooting..."
  sleep 1

done

# --private-key-file "$DATA_PATH/key" \