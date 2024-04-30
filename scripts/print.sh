#!/usr/bin/env bash

echo "0: $(curl -s http://localhost:3030/mainnet/block/height/latest)"
echo "1: $(curl -s http://localhost:3031/mainnet/block/height/latest)"
echo "2: $(curl -s http://localhost:3032/mainnet/block/height/latest)"
echo "3: $(curl -s http://localhost:3033/mainnet/block/height/latest)"
