#!/usr/bin/env bash

if [[ -n "$1" ]]
then
  TEST_PATH="$(pwd)/tests/$1"
else
  TEST_PATH="$(pwd)/tests/$(ls tests | sort | tail -n1)"
fi

if [[ ! -d $TEST_PATH ]]
then
  exit 1
fi

echo $TEST_PATH