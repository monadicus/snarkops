#!/usr/bin/env bash

cargo watch -x 'run -p snops' \
  -w ./crates/snops \
  -w ./crates/common \
  -w ./crates/checkpoint