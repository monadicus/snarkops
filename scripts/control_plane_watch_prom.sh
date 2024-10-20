#!/usr/bin/env bash

cargo watch -x 'run -p snops -- --prometheus http://127.0.0.1:9090 --loki http://127.0.0.1:3100' \
  -w ./crates/snops \
  -w ./crates/snops-common \
  -w ./crates/snops-checkpoint
