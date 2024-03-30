#!/usr/bin/env bash

# check if index is set
if [ -z "$1" ]; then
  echo "Usage: $0 <id>"
  exit 1
fi

curl -v -X POST http://localhost:1234/api/v1/env/$1
