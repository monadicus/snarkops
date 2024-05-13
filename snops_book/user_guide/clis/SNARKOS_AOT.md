# Command-Line Help for `snarkos-aot`

This document contains the help content for the `snarkos-aot` command-line program.

**Command Overview:**

* [`snarkos-aot`↴](#snarkos-aot)
* [`snarkos-aot genesis`↴](#snarkos-aot-genesis)
* [`snarkos-aot accounts`↴](#snarkos-aot-accounts)
* [`snarkos-aot ledger`↴](#snarkos-aot-ledger)
* [`snarkos-aot ledger init`↴](#snarkos-aot-ledger-init)
* [`snarkos-aot ledger tx`↴](#snarkos-aot-ledger-tx)
* [`snarkos-aot ledger tx from-ops`↴](#snarkos-aot-ledger-tx-from-ops)
* [`snarkos-aot ledger tx num`↴](#snarkos-aot-ledger-tx-num)
* [`snarkos-aot ledger add`↴](#snarkos-aot-ledger-add)
* [`snarkos-aot ledger add random`↴](#snarkos-aot-ledger-add-random)
* [`snarkos-aot ledger add stdin`↴](#snarkos-aot-ledger-add-stdin)
* [`snarkos-aot ledger view`↴](#snarkos-aot-ledger-view)
* [`snarkos-aot ledger view top`↴](#snarkos-aot-ledger-view-top)
* [`snarkos-aot ledger view block`↴](#snarkos-aot-ledger-view-block)
* [`snarkos-aot ledger view balance`↴](#snarkos-aot-ledger-view-balance)
* [`snarkos-aot ledger view records`↴](#snarkos-aot-ledger-view-records)
* [`snarkos-aot ledger distribute`↴](#snarkos-aot-ledger-distribute)
* [`snarkos-aot ledger rewind`↴](#snarkos-aot-ledger-rewind)
* [`snarkos-aot ledger replay`↴](#snarkos-aot-ledger-replay)
* [`snarkos-aot ledger execute`↴](#snarkos-aot-ledger-execute)
* [`snarkos-aot ledger query`↴](#snarkos-aot-ledger-query)
* [`snarkos-aot ledger hash`↴](#snarkos-aot-ledger-hash)
* [`snarkos-aot ledger checkpoint`↴](#snarkos-aot-ledger-checkpoint)
* [`snarkos-aot ledger checkpoint create`↴](#snarkos-aot-ledger-checkpoint-create)
* [`snarkos-aot ledger checkpoint apply`↴](#snarkos-aot-ledger-checkpoint-apply)
* [`snarkos-aot ledger checkpoint view`↴](#snarkos-aot-ledger-checkpoint-view)
* [`snarkos-aot ledger checkpoint clean`↴](#snarkos-aot-ledger-checkpoint-clean)
* [`snarkos-aot run`↴](#snarkos-aot-run)
* [`snarkos-aot execute`↴](#snarkos-aot-execute)
* [`snarkos-aot authorize`↴](#snarkos-aot-authorize)
* [`snarkos-aot authorize transfer-public`↴](#snarkos-aot-authorize-transfer-public)
* [`snarkos-aot man`↴](#snarkos-aot-man)
* [`snarkos-aot md`↴](#snarkos-aot-md)

## `snarkos-aot`

**Usage:** `snarkos-aot [OPTIONS] <COMMAND>`

###### **Subcommands:**

* `genesis` — 
* `accounts` — 
* `ledger` — 
* `run` — 
* `execute` — 
* `authorize` — 
* `man` — For generating cli manpages. Only with the mangen feature enabled
* `md` — For generating cli markdown. Only with the clipages feature enabled

###### **Options:**

* `--enable-profiling`

  Possible values: `true`, `false`

* `--log <LOG>`
* `--verbosity <VERBOSITY>`

  Default value: `4`
* `--loki <LOKI>`



## `snarkos-aot genesis`

**Usage:** `snarkos-aot genesis [OPTIONS]`

###### **Options:**

* `-g`, `--genesis-key <genesis-key>` — The private key to use when generating the genesis block. Generates one randomly if not passed
* `-o`, `--output <output>` — Where to write the genesis block to

  Default value: `genesis.block`
* `--committee-size <committee-size>` — The committee size. Not used if --bonded-balances is set

  Default value: `4`
* `--committee-output <committee-output>` — A place to optionally write out the generated committee private keys JSON
* `--additional-accounts <additional-accounts>` — Additional number of accounts that aren't validators to add balances to

  Default value: `0`
* `--additional-accounts-balance <additional-accounts-balance>` — The balance to add to the number of accounts specified by additional-accounts

  Default value: `100000000`
* `--additional-accounts-record-balance <additional-accounts-record-balance>` — If --additional-accounts is passed you can additionally add an amount to give them in a record
* `--additional-accounts-output <additional-accounts-output>` — A place to write out the additionally generated accounts by --additional-accounts
* `--seed <seed>` — The seed to use when generating committee private keys and the genesis block. If unpassed, uses DEVELOPMENT_MODE_RNG_SEED (1234567890u64)
* `--bonded-balance <bonded-balance>` — The bonded balance each bonded address receives. Not used if `--bonded-balances` is passed

  Default value: `10000000000000`
* `--bonded-balances <bonded-balances>` — An optional map from address to bonded balance. Overrides `--bonded-balance` and `--committee-size`
* `--ledger <ledger>` — Optionally initialize a ledger as well



## `snarkos-aot accounts`

**Usage:** `snarkos-aot accounts [OPTIONS] <COUNT>`

###### **Arguments:**

* `<COUNT>` — Number of accounts to generate

###### **Options:**

* `-o`, `--output <OUTPUT>` — Where to write the output to
* `-s`, `--seed <seed>` — The seed to use when generating private keys If unpassed, uses a random seed



## `snarkos-aot ledger`

**Usage:** `snarkos-aot ledger [OPTIONS] --genesis <GENESIS> --ledger <LEDGER> <COMMAND>`

###### **Subcommands:**

* `init` — 
* `tx` — 
* `add` — 
* `view` — 
* `distribute` — 
* `rewind` — 
* `replay` — 
* `execute` — 
* `query` — Receive inquiries on /mainnet/latest/stateRoot
* `hash` — 
* `checkpoint` — 

###### **Options:**

* `--enable-profiling`

  Possible values: `true`, `false`

* `-g`, `--genesis <GENESIS>` — A path to the genesis block to initialize the ledger from

  Default value: `./genesis.block`
* `-l`, `--ledger <LEDGER>` — The ledger from which to view a block

  Default value: `./ledger`



## `snarkos-aot ledger init`

**Usage:** `snarkos-aot ledger init`



## `snarkos-aot ledger tx`

**Usage:** `snarkos-aot ledger tx <COMMAND>`

###### **Subcommands:**

* `from-ops` — 
* `num` — 



## `snarkos-aot ledger tx from-ops`

**Usage:** `snarkos-aot ledger tx from-ops --operations <OPERATIONS>`

###### **Options:**

* `--operations <OPERATIONS>`



## `snarkos-aot ledger tx num`

**Usage:** `snarkos-aot ledger tx num --private-keys <PRIVATE_KEYS> <NUM>`

###### **Arguments:**

* `<NUM>`

###### **Options:**

* `--private-keys <PRIVATE_KEYS>`



## `snarkos-aot ledger add`

**Usage:** `snarkos-aot ledger add <COMMAND>`

###### **Subcommands:**

* `random` — 
* `stdin` — 



## `snarkos-aot ledger add random`

**Usage:** `snarkos-aot ledger add random [OPTIONS] --private-keys <PRIVATE_KEYS>`

###### **Options:**

* `--block-private-key <BLOCK_PRIVATE_KEY>`
* `--private-keys <PRIVATE_KEYS>`
* `-n`, `--num-blocks <NUM_BLOCKS>`

  Default value: `5`
* `--min-per-block <MIN_PER_BLOCK>` — Minimum number of transactions per block

  Default value: `128`
* `--max-per-block <MAX_PER_BLOCK>` — Maximumnumber of transactions per block

  Default value: `1024`
* `--max-tx-credits <MAX_TX_CREDITS>` — Maximum transaction credit transfer. If unspecified, maximum is entire account balance



## `snarkos-aot ledger add stdin`

**Usage:** `snarkos-aot ledger add stdin [OPTIONS]`

###### **Options:**

* `--private-key <private-key>` — The private key to use when generating the block
* `--txs-per-block <txs-per-block>` — The number of transactions to add per block



## `snarkos-aot ledger view`

**Usage:** `snarkos-aot ledger view <COMMAND>`

###### **Subcommands:**

* `top` — 
* `block` — 
* `balance` — 
* `records` — 



## `snarkos-aot ledger view top`

**Usage:** `snarkos-aot ledger view top`



## `snarkos-aot ledger view block`

**Usage:** `snarkos-aot ledger view block <BLOCK_HEIGHT>`

###### **Arguments:**

* `<BLOCK_HEIGHT>`



## `snarkos-aot ledger view balance`

**Usage:** `snarkos-aot ledger view balance <ADDRESS>`

###### **Arguments:**

* `<ADDRESS>`



## `snarkos-aot ledger view records`

**Usage:** `snarkos-aot ledger view records <PRIVATE_KEY>`

###### **Arguments:**

* `<PRIVATE_KEY>`



## `snarkos-aot ledger distribute`

**Usage:** `snarkos-aot ledger distribute [OPTIONS] --from <FROM> --amount <AMOUNT>`

###### **Options:**

* `--from <FROM>` — The private key in which to distribute credits from
* `--to <TO>` — A comma-separated list of addresses to distribute credits to. This or `--num-accounts` must be passed
* `--num-accounts <NUM_ACCOUNTS>` — The number of new addresses to generate and distribute credits to. This or `--to` must be passed
* `--amount <AMOUNT>` — The amount of microcredits to distribute



## `snarkos-aot ledger rewind`

**Usage:** `snarkos-aot ledger rewind <CHECKPOINT>`

###### **Arguments:**

* `<CHECKPOINT>`



## `snarkos-aot ledger replay`

**Usage:** `snarkos-aot ledger replay [OPTIONS]`

###### **Options:**

* `--height <HEIGHT>`
* `--amount <AMOUNT>`
* `--skip <SKIP>` — How many blocks to skip when reading

  Default value: `1`
* `-c`, `--checkpoint` — When checkpoint is enabled, checkpoints

  Default value: `false`

  Possible values: `true`, `false`




## `snarkos-aot ledger execute`

**Usage:** `snarkos-aot ledger execute --query <QUERY> <AUTHORIZATION>`

###### **Arguments:**

* `<AUTHORIZATION>`

###### **Options:**

* `-q`, `--query <QUERY>`



## `snarkos-aot ledger query`

Receive inquiries on /mainnet/latest/stateRoot

**Usage:** `snarkos-aot ledger query [OPTIONS]`

###### **Options:**

* `--port <PORT>` — Port to listen on for incoming messages

  Default value: `3030`
* `--bind <BIND>`

  Default value: `0.0.0.0`
* `--readonly` — When true, the POST /block endpoint will not be available

  Possible values: `true`, `false`

* `--record` — Receive messages from /mainnet/transaction/broadcast and record them to the output

  Possible values: `true`, `false`

* `-o`, `--output <OUTPUT>` — Path to the directory containing the stored data

  Default value: `transactions.json`



## `snarkos-aot ledger hash`

**Usage:** `snarkos-aot ledger hash`



## `snarkos-aot ledger checkpoint`

**Usage:** `snarkos-aot ledger checkpoint <COMMAND>`

###### **Subcommands:**

* `create` — Create a checkpoint for the given ledger
* `apply` — Apply a checkpoint to the given ledger
* `view` — View the available checkpoints
* `clean` — Cleanup old checkpoints



## `snarkos-aot ledger checkpoint create`

Create a checkpoint for the given ledger

**Usage:** `snarkos-aot ledger checkpoint create`



## `snarkos-aot ledger checkpoint apply`

Apply a checkpoint to the given ledger

**Usage:** `snarkos-aot ledger checkpoint apply [OPTIONS] <CHECKPOINT>`

###### **Arguments:**

* `<CHECKPOINT>` — Checkpoint file to apply

###### **Options:**

* `-c`, `--clean` — When present, clean up old checkpoints that are no longer applicable after applying the checkpoint

  Default value: `false`

  Possible values: `true`, `false`




## `snarkos-aot ledger checkpoint view`

View the available checkpoints

**Usage:** `snarkos-aot ledger checkpoint view`



## `snarkos-aot ledger checkpoint clean`

Cleanup old checkpoints

**Usage:** `snarkos-aot ledger checkpoint clean`



## `snarkos-aot run`

**Usage:** `snarkos-aot run [OPTIONS] --genesis <GENESIS> --ledger <LEDGER> --type <type> <--private-key <PRIVATE_KEY>|--private-key-file <PRIVATE_KEY_FILE>>`

###### **Options:**

* `-g`, `--genesis <GENESIS>` — A path to the genesis block to initialize the ledger from

  Default value: `genesis.block`
* `-l`, `--ledger <LEDGER>` — The ledger from which to view a block

  Default value: `./ledger`
* `-t`, `--type <type>`
* `--private-key <PRIVATE_KEY>` — Specify the account private key of the node
* `--private-key-file <PRIVATE_KEY_FILE>` — Specify the account private key of the node
* `--bind <BIND_ADDR>`

  Default value: `0.0.0.0`
* `--node <NODE>` — Specify the IP address and port for the node server

  Default value: `4130`
* `--bft <BFT>` — Specify the IP address and port for the BFT

  Default value: `5000`
* `--rest <REST>` — Specify the IP address and port for the REST server

  Default value: `3030`
* `--metrics <METRICS>` — Specify the port for the metrics server

  Default value: `9000`
* `--peers <PEERS>` — Specify the IP address and port of the peer(s) to connect to

  Default value: ``
* `--validators <VALIDATORS>` — Specify the IP address and port of the validator(s) to connect to

  Default value: ``
* `--rest-rps <REST_RPS>` — Specify the requests per second (RPS) rate limit per IP for the REST server

  Default value: `1000`
* `--retention-policy <RETENTION_POLICY>`



## `snarkos-aot execute`

**Usage:** `snarkos-aot execute --query <QUERY> <AUTHORIZATION>`

###### **Arguments:**

* `<AUTHORIZATION>`

###### **Options:**

* `-q`, `--query <QUERY>`



## `snarkos-aot authorize`

**Usage:** `snarkos-aot authorize <COMMAND>`

###### **Subcommands:**

* `transfer-public` — 



## `snarkos-aot authorize transfer-public`

**Usage:** `snarkos-aot authorize transfer-public [OPTIONS] --private-key <PRIVATE_KEY> --recipient <RECIPIENT> --amount <AMOUNT>`

###### **Options:**

* `--private-key <PRIVATE_KEY>`
* `--recipient <RECIPIENT>`
* `-a`, `--amount <AMOUNT>`
* `--priority-fee <PRIORITY_FEE>`

  Default value: `0`
* `--broadcast`

  Default value: `false`

  Possible values: `true`, `false`




## `snarkos-aot man`

For generating cli manpages. Only with the mangen feature enabled

**Usage:** `snarkos-aot man [DIRECTORY]`

###### **Arguments:**

* `<DIRECTORY>`

  Default value: `target/man/snops-cli`



## `snarkos-aot md`

For generating cli markdown. Only with the clipages feature enabled

**Usage:** `snarkos-aot md [DIRECTORY]`

###### **Arguments:**

* `<DIRECTORY>`

  Default value: `snops_book/user_guide/clis`



<hr/>

<small><i>
    This document was generated automatically by
    <a href="https://crates.io/crates/clap-markdown"><code>clap-markdown</code></a>.
</i></small>