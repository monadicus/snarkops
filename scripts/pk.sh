#!/usr/bin/env bash

BINARY="$(pwd)/target/release/snarkos-aot"

TEST_PATH=$(scripts/test_path.sh)
COMMITTEE=$TEST_PATH/committee.json

cat $COMMITTEE | jq "[.[][0]][$1]" -r;