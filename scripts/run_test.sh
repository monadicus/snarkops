#!/usr/bin/env bash

# check if index is set
if [ ! -f "$1" ]; then
  echo "Usage: $0 <test.yaml>"
  exit 1
fi

curl http://localhost:1234/api/v1/test/prepare -d "$(cat $1)"
