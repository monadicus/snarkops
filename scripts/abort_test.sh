#!/usr/bin/env bash

curl 'http://127.0.0.1:1234/api/v1/env' -X DELETE

# TODO: specify the test to abort

# # check if index is set
# if [ ! -f "$1" ]; then
#   echo "Usage: $0 <test.yaml>"
#   exit 1
# fi
# curl -H "Content-Type: application/json" http://localhost:1234/api/v1/test/prepare -d "$(cat $1)"
