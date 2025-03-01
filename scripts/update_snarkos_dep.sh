#!/usr/bin/env bash

source scripts/dep_utils.sh

get_snarkos_rev $1 || exit 1

echo -e "${GREEN}>>${NC} Found ref ${YELLOW}$SNARKOS_REF_ID${NC} with commit hash ${YELLOW}$SNARKOS_COMMIT_HASH${NC}"
echo -e "${GREEN}>>${NC} Found ${GREEN}snarkOS${NC} rev ${YELLOW}$SNARKOS_REV${NC}"
echo -e "${GREEN}>>${NC} Found ${GREEN}snarkVM${NC} rev ${YELLOW}$SNARKVM_REV${NC}"

echo -e "${GREEN}>>${NC} Updating ${GREEN}Cargo.toml${NC} with respective versions."

# Replace local Cargo.toml with the one from the target ref
sed -i -E "s/(snarkOS\", rev = \")([^\"]*)/\1$SNARKOS_REV/" Cargo.toml

# Copy the snarkvm dependency lines from snarkOS' source Cargo.toml to the local Cargo.toml
sed -i -e '/## CODEGEN_START/,/## CODEGEN_END/{/## CODEGEN_START/!{/## CODEGEN_END/!d}}' \
  -e '/## CODEGEN_START/ r /dev/stdin' Cargo.toml <<< "$SNARKVM_CARGO_DATA"
