#!/usr/bin/env bash

ENDPOINT="$1"
NUM_BLOCKS="$2"

# check if mode is client, validator, or prover
if [ -z "$ENDPOINT" ]; then
  echo "usage: $0 <http://127.0.0.1:3030> [num_blocks]"
  exit 1
fi

# check if index is set
if [ -z "$NUM_BLOCKS" ]; then
  NUM_BLOCKS=10
fi

HEIGHT="$(curl -s $ENDPOINT/mainnet/latest/height)"
if [ -z "$HEIGHT" ]; then
  echo "error: failed to get height from $ENDPOINT"
  exit 1
fi

if [ "$HEIGHT" -lt "$NUM_BLOCKS" ]; then
  echo "error: ledger is shorter than test range ($HEIGHT < $NUM_BLOCKS)"
  exit 1
fi

if [ "$NUM_BLOCKS" -lt 2 ]; then
  echo "error: [num_blocks] must be at least 2"
  exit 1
fi


_YEL=$(tput setaf 3)
_RES=$(tput sgr0)
_BLD=$(tput bold)

echo "fetching blocks $_YEL$((HEIGHT-NUM_BLOCKS+1))$_RES to $_YEL$HEIGHT$_RES"

TOTAL_TX=0
TOTAL_BLOCKS=0
FIRST_TIMESTAMP=""
LAST_TIMESTAMP=""
MIN_BLOCK_TIME=999
MAX_BLOCK_TIME=0

prev_block_time=""

# get all the blocks from (HEIGHT-NUM_BLOCKS) to HEIGHT
for i in $(seq $((HEIGHT-NUM_BLOCKS+1)) $HEIGHT); do
  block="$(curl -s $ENDPOINT/mainnet/block/$i)"

  # update timestamps
  LAST_TIMESTAMP="$(echo $block | jq '.header.metadata.timestamp')"
  if [ -z "$FIRST_TIMESTAMP" ]; then
    FIRST_TIMESTAMP="$LAST_TIMESTAMP"
    prev_block_time="$LAST_TIMESTAMP"
  else
    # only increment transactions if this is not the first block
    # because the first block is the timestamp for the next block's transactions
    num_tx=$(echo $block | jq '.transactions | length')
    TOTAL_TX=$(expr $num_tx + $TOTAL_TX)
    TOTAL_BLOCKS=$(expr $TOTAL_BLOCKS + 1)

    # calculate min/max block times
    block_time=$(expr $LAST_TIMESTAMP - $prev_block_time)
    if [ "$block_time" -lt "$MIN_BLOCK_TIME" ]; then
      MIN_BLOCK_TIME=$block_time
    fi
    if [ "$block_time" -gt "$MAX_BLOCK_TIME" ]; then
      MAX_BLOCK_TIME=$block_time
    fi
    prev_block_time=$LAST_TIMESTAMP
  fi
done

SPAN=$(expr $LAST_TIMESTAMP - $FIRST_TIMESTAMP)

echo " ${_BLD}Duration$_RES"
echo "          first block: $(date -d @$FIRST_TIMESTAMP)"
echo "           last block: $(date -d @$LAST_TIMESTAMP)"
echo "           total time: $_YEL$SPAN seconds$_RES"
echo "       min block time: $_YEL$MIN_BLOCK_TIME seconds$_RES"
echo "       max block time: $_YEL$MAX_BLOCK_TIME seconds$_RES"

echo ""
echo " ${_BLD}Blocks$_RES"
echo "    total block count: $_YEL$TOTAL_BLOCKS$_RES"
echo "       avg block time: $_YEL$(awk "BEGIN{print $SPAN / $TOTAL_BLOCKS}") seconds$_RES"
echo ""
echo " ${_BLD}Transactions$_RES"
echo "   total transactions: $_YEL$TOTAL_TX$_RES"
echo "              avg tps: $_YEL$(awk "BEGIN{print $TOTAL_TX / $SPAN}") tx/s$_RES"
