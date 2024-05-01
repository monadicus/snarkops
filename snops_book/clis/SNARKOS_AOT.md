# Command-Line Help for `snarkOS AoT`

This document contains the help content for the `snarkOS AoT` command-line program.

**Command Overview:**

* [`snarkOS AoT`↴](#snarkOS AoT)
* [`snarkOS AoT genesis`↴](#snarkOS AoT-genesis)
* [`snarkOS AoT accounts`↴](#snarkOS AoT-accounts)
* [`snarkOS AoT ledger`↴](#snarkOS AoT-ledger)
* [`snarkOS AoT ledger init`↴](#snarkOS AoT-ledger-init)
* [`snarkOS AoT ledger tx`↴](#snarkOS AoT-ledger-tx)
* [`snarkOS AoT ledger tx from-ops`↴](#snarkOS AoT-ledger-tx-from-ops)
* [`snarkOS AoT ledger tx num`↴](#snarkOS AoT-ledger-tx-num)
* [`snarkOS AoT ledger add`↴](#snarkOS AoT-ledger-add)
* [`snarkOS AoT ledger add random`↴](#snarkOS AoT-ledger-add-random)
* [`snarkOS AoT ledger add stdin`↴](#snarkOS AoT-ledger-add-stdin)
* [`snarkOS AoT ledger view`↴](#snarkOS AoT-ledger-view)
* [`snarkOS AoT ledger view top`↴](#snarkOS AoT-ledger-view-top)
* [`snarkOS AoT ledger view block`↴](#snarkOS AoT-ledger-view-block)
* [`snarkOS AoT ledger view balance`↴](#snarkOS AoT-ledger-view-balance)
* [`snarkOS AoT ledger view records`↴](#snarkOS AoT-ledger-view-records)
* [`snarkOS AoT ledger distribute`↴](#snarkOS AoT-ledger-distribute)
* [`snarkOS AoT ledger rewind`↴](#snarkOS AoT-ledger-rewind)
* [`snarkOS AoT ledger replay`↴](#snarkOS AoT-ledger-replay)
* [`snarkOS AoT ledger execute`↴](#snarkOS AoT-ledger-execute)
* [`snarkOS AoT ledger query`↴](#snarkOS AoT-ledger-query)
* [`snarkOS AoT ledger hash`↴](#snarkOS AoT-ledger-hash)
* [`snarkOS AoT ledger checkpoint`↴](#snarkOS AoT-ledger-checkpoint)
* [`snarkOS AoT ledger checkpoint create`↴](#snarkOS AoT-ledger-checkpoint-create)
* [`snarkOS AoT ledger checkpoint apply`↴](#snarkOS AoT-ledger-checkpoint-apply)
* [`snarkOS AoT ledger checkpoint view`↴](#snarkOS AoT-ledger-checkpoint-view)
* [`snarkOS AoT ledger checkpoint clean`↴](#snarkOS AoT-ledger-checkpoint-clean)
* [`snarkOS AoT run`↴](#snarkOS AoT-run)
* [`snarkOS AoT execute`↴](#snarkOS AoT-execute)
* [`snarkOS AoT authorize`↴](#snarkOS AoT-authorize)
* [`snarkOS AoT authorize transfer-public`↴](#snarkOS AoT-authorize-transfer-public)
* [`snarkOS AoT man`↴](#snarkOS AoT-man)
* [`snarkOS AoT md`↴](#snarkOS AoT-md)

## `snarkOS AoT`

**Usage:** `snarkOS AoT [OPTIONS] <COMMAND>`

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



## `snarkOS AoT genesis`

**Usage:** `snarkOS AoT genesis [OPTIONS]`

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



## `snarkOS AoT accounts`

**Usage:** `snarkOS AoT accounts [OPTIONS] <COUNT>`

###### **Arguments:**

* `<COUNT>` — Number of accounts to generate

###### **Options:**

* `-o`, `--output <OUTPUT>` — Where to write the output to
* `-s`, `--seed <seed>` — The seed to use when generating private keys If unpassed, uses a random seed



## `snarkOS AoT ledger`

**Usage:** `snarkOS AoT ledger [OPTIONS] --genesis <GENESIS> --ledger <LEDGER> <COMMAND>`

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



## `snarkOS AoT ledger init`

**Usage:** `snarkOS AoT ledger init`



## `snarkOS AoT ledger tx`

**Usage:** `snarkOS AoT ledger tx <COMMAND>`

###### **Subcommands:**

* `from-ops` — 
* `num` — 



## `snarkOS AoT ledger tx from-ops`

**Usage:** `snarkOS AoT ledger tx from-ops --operations <OPERATIONS>`

###### **Options:**

* `--operations <OPERATIONS>`



## `snarkOS AoT ledger tx num`

**Usage:** `snarkOS AoT ledger tx num --private-keys <PRIVATE_KEYS> <NUM>`

###### **Arguments:**

* `<NUM>`

###### **Options:**

* `--private-keys <PRIVATE_KEYS>`



## `snarkOS AoT ledger add`

**Usage:** `snarkOS AoT ledger add <COMMAND>`

###### **Subcommands:**

* `random` — 
* `stdin` — 



## `snarkOS AoT ledger add random`

**Usage:** `snarkOS AoT ledger add random [OPTIONS] --private-keys <PRIVATE_KEYS>`

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



## `snarkOS AoT ledger add stdin`

**Usage:** `snarkOS AoT ledger add stdin [OPTIONS]`

###### **Options:**

* `--private-key <private-key>` — The private key to use when generating the block
* `--txs-per-block <txs-per-block>` — The number of transactions to add per block



## `snarkOS AoT ledger view`

**Usage:** `snarkOS AoT ledger view <COMMAND>`

###### **Subcommands:**

* `top` — 
* `block` — 
* `balance` — 
* `records` — 



## `snarkOS AoT ledger view top`

**Usage:** `snarkOS AoT ledger view top`



## `snarkOS AoT ledger view block`

**Usage:** `snarkOS AoT ledger view block <BLOCK_HEIGHT>`

###### **Arguments:**

* `<BLOCK_HEIGHT>`



## `snarkOS AoT ledger view balance`

**Usage:** `snarkOS AoT ledger view balance <ADDRESS>`

###### **Arguments:**

* `<ADDRESS>`



## `snarkOS AoT ledger view records`

**Usage:** `snarkOS AoT ledger view records <PRIVATE_KEY>`

###### **Arguments:**

* `<PRIVATE_KEY>`



## `snarkOS AoT ledger distribute`

**Usage:** `snarkOS AoT ledger distribute [OPTIONS] --from <FROM> --amount <AMOUNT>`

###### **Options:**

* `--from <FROM>` — The private key in which to distribute credits from
* `--to <TO>` — A comma-separated list of addresses to distribute credits to. This or `--num-accounts` must be passed
* `--num-accounts <NUM_ACCOUNTS>` — The number of new addresses to generate and distribute credits to. This or `--to` must be passed
* `--amount <AMOUNT>` — The amount of microcredits to distribute



## `snarkOS AoT ledger rewind`

**Usage:** `snarkOS AoT ledger rewind <CHECKPOINT>`

###### **Arguments:**

* `<CHECKPOINT>`



## `snarkOS AoT ledger replay`

**Usage:** `snarkOS AoT ledger replay [OPTIONS]`

###### **Options:**

* `--height <HEIGHT>`
* `--amount <AMOUNT>`
* `--skip <SKIP>` — How many blocks to skip when reading

  Default value: `1`
* `-c`, `--checkpoint` — When checkpoint is enabled, checkpoints

  Default value: `false`

  Possible values: `true`, `false`




## `snarkOS AoT ledger execute`

**Usage:** `snarkOS AoT ledger execute --query <QUERY> <AUTHORIZATION>`

###### **Arguments:**

* `<AUTHORIZATION>`

###### **Options:**

* `-q`, `--query <QUERY>`



## `snarkOS AoT ledger query`

Receive inquiries on /mainnet/latest/stateRoot

**Usage:** `snarkOS AoT ledger query [OPTIONS]`

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



## `snarkOS AoT ledger hash`

**Usage:** `snarkOS AoT ledger hash`



## `snarkOS AoT ledger checkpoint`

**Usage:** `snarkOS AoT ledger checkpoint <COMMAND>`

###### **Subcommands:**

* `create` — Create a checkpoint for the given ledger
* `apply` — Apply a checkpoint to the given ledger
* `view` — View the available checkpoints
* `clean` — Cleanup old checkpoints



## `snarkOS AoT ledger checkpoint create`

Create a checkpoint for the given ledger

**Usage:** `snarkOS AoT ledger checkpoint create`



## `snarkOS AoT ledger checkpoint apply`

Apply a checkpoint to the given ledger

**Usage:** `snarkOS AoT ledger checkpoint apply [OPTIONS] <CHECKPOINT>`

###### **Arguments:**

* `<CHECKPOINT>` — Checkpoint file to apply

###### **Options:**

* `-c`, `--clean` — When present, clean up old checkpoints that are no longer applicable after applying the checkpoint

  Default value: `false`

  Possible values: `true`, `false`




## `snarkOS AoT ledger checkpoint view`

View the available checkpoints

**Usage:** `snarkOS AoT ledger checkpoint view`



## `snarkOS AoT ledger checkpoint clean`

Cleanup old checkpoints

**Usage:** `snarkOS AoT ledger checkpoint clean`



## `snarkOS AoT run`

**Usage:** `snarkOS AoT run [OPTIONS] --genesis <GENESIS> --ledger <LEDGER> --type <type> <--private-key <PRIVATE_KEY>|--private-key-file <PRIVATE_KEY_FILE>>`

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



## `snarkOS AoT execute`

**Usage:** `snarkOS AoT execute --query <QUERY> <AUTHORIZATION>`

###### **Arguments:**

* `<AUTHORIZATION>`

###### **Options:**

* `-q`, `--query <QUERY>`



## `snarkOS AoT authorize`

**Usage:** `snarkOS AoT authorize <COMMAND>`

###### **Subcommands:**

* `transfer-public` — 



## `snarkOS AoT authorize transfer-public`

**Usage:** `snarkOS AoT authorize transfer-public [OPTIONS] --private-key <PRIVATE_KEY> --recipient <RECIPIENT> --amount <AMOUNT>`

###### **Options:**

* `--private-key <PRIVATE_KEY>`
* `--recipient <RECIPIENT>`
* `-a`, `--amount <AMOUNT>`
* `--priority-fee <PRIORITY_FEE>`

  Default value: `0`
* `--broadcast`

  Default value: `false`

  Possible values: `true`, `false`




## `snarkOS AoT man`

For generating cli manpages. Only with the mangen feature enabled

**Usage:** `snarkOS AoT man [DIRECTORY]`

###### **Arguments:**

* `<DIRECTORY>`

  Default value: `target/man/snops-cli`



## `snarkOS AoT md`

For generating cli markdown. Only with the clipages feature enabled

**Usage:** `snarkOS AoT md [DIRECTORY]`

###### **Arguments:**

* `<DIRECTORY>`

  Default value: `snops_book/clis`



<hr/>

<small><i>
    This document was generated automatically by
    <a href="https://crates.io/crates/clap-markdown"><code>clap-markdown</code></a>.
</i></small>
