#!/usr/bin/env bash

docker compose -f ./scripts/metrics/docker-compose.yaml up -d --force-recreate
