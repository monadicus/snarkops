#!/usr/bin/env bash

cargo watch -x 'build --profile release-big -p snops-agent' \
  -w ./crates/agent \
  -w ./crates/common \
  -w ./crates/checkpoint