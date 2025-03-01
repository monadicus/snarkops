#!/usr/bin/env bash

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
NC='\033[0m'

function errecho() {
  echo -e "$@" >>/dev/stderr
}

function get_snarkos_rev() {
  # use $1 or 'staging'
  local target_label=${1:-staging}

  # Look for ref/tags/$target_label or ref/heads/$target_label
  local refs="$(git ls-remote --heads --tags https://github.com/ProvableHQ/snarkOS.git 2>/dev/null | grep -E "refs/(heads|tags)/$target_label")"

  if [ -z "$refs" ]; then
    errecho "${RED}>>${NC} No tags or branch '$target_label' found"
    exit 1
  fi

  # Split ref data by tab character into SNARKOS_COMMIT_HASH and SNARKOS_REF_ID
  IFS=$'\t' read -r SNARKOS_COMMIT_HASH SNARKOS_REF_ID <<< "$refs"

  # Get snarkOS rev from Cargo.toml
  local cargo_toml_url="https://raw.githubusercontent.com/ProvableHQ/snarkOS/$SNARKOS_REF_ID/Cargo.toml"
  SNARKOS_CARGO_TOML_DATA="$(curl -fksSL "$cargo_toml_url" -o-)"
  SNARKVM_REV="$(echo "$SNARKOS_CARGO_TOML_DATA" | sed -nE 's/#?rev = "([^"]+)"/\1/p')"
  # truncate the rev to 7 characters
  SNARKOS_REV="${SNARKOS_COMMIT_HASH:0:7}"

  if [ -z "$SNARKVM_REV" ]; then
    errecho "${RED}>>${NC} Could not find snarkVM rev in Cargo.toml for $SNARKOS_REF_ID"
    exit 1
  fi

  # Read the lines '[workspace.dependencies.snarkvm]' to ^features greedily
  # Excluding the first line '[workspace.dependencies.snarkvm]' and the last line '^features'
  SNARKVM_CARGO_DATA="$(echo "$SNARKOS_CARGO_TOML_DATA" | sed -nE '/\[workspace\.dependencies\.snarkvm\]/,/^features/ { /^features/!p }' | sed '1d')"
}

