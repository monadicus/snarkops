#!/usr/bin/env bash

cargo watch -x 'build --profile release-big -p snops-agent' \
  -w ./crates/snops-agent \
  -w ./crates/snops-common \
  -w ./crates/snops-checkpoint \
  -w ./crates/snops-node