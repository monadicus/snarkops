#!/usr/bin/env bash


PROFILE="release-big"
echo "AOT_BIN = $(pwd)/target/$PROFILE/snarkos-aot"
echo "AGENT_BIN = $(pwd)/target/$PROFILE/snops-agent"

AOT_BIN="$(pwd)/target/$PROFILE/snarkos-aot" \
AGENT_BIN="$(pwd)/target/$PROFILE/snops-agent" \
cargo watch -x 'run -p snops' \
  -w ./crates/snops \
  -w ./crates/snops-common \
  -w ./crates/checkpoint