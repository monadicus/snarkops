#!/usr/bin/env bash

cargo watch -x 'run -p snops' \
  -w ./crates/snops \
  -w ./crates/snops-common \
  -w ./crates/snops-checkpoint