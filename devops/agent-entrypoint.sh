#!/usr/bin/env bash

SNOPS_AGENT_DATA_DIR=/var/opt/snops-agent
AGENT_BIN=/etc/snops-agent/agent

if [ ! $SNOPS_ENDPOINT ]; then
  echo "SNOPS_ENDPOINT is not set"
  exit 1
fi

if [ ! $SNOPS_AGENT_ID ]; then
  echo "SNOPS_AGENT_ID is not set"
  exit 1
fi

function download_agent() {
  # conditionally set the -z flag to check if the file exists
  if [ -e $AGENT_BIN ]; then
    zflag="-z '$AGENT_BIN'"
  else
    zflag=
  fi

  # Download the agent binary
  curl -sSL "$SNOPS_ENDPOINT/content/agent" $zflag -o $AGENT_BIN
  chmod +x $AGENT_BIN
}

echo "Using endpoint: $SNOPS_ENDPOINT"

sleep 1

while true; do
  download_agent
  $AGENT_BIN --labels "k8s,$AGENT_LABELS"
  echo "Agent exited, restarting in 5 seconds..."
  sleep 5
done
