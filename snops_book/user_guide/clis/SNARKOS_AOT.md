# Command-Line Help for `snarkos-aot`

This document contains the help content for the `snarkos-aot` command-line program.

**Command Overview:**

* [`snarkos-aot`↴](#snarkos-aot)
* [`snarkos-aot genesis`↴](#snarkos-aot-genesis)
* [`snarkos-aot accounts`↴](#snarkos-aot-accounts)
* [`snarkos-aot ledger`↴](#snarkos-aot-ledger)
* [`snarkos-aot ledger init`↴](#snarkos-aot-ledger-init)
* [`snarkos-aot ledger view`↴](#snarkos-aot-ledger-view)
* [`snarkos-aot ledger view top`↴](#snarkos-aot-ledger-view-top)
* [`snarkos-aot ledger view block`↴](#snarkos-aot-ledger-view-block)
* [`snarkos-aot ledger view balance`↴](#snarkos-aot-ledger-view-balance)
* [`snarkos-aot ledger view records`↴](#snarkos-aot-ledger-view-records)
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
* [`snarkos-aot auth`↴](#snarkos-aot-auth)
* [`snarkos-aot auth execute`↴](#snarkos-aot-auth-execute)
* [`snarkos-aot auth program`↴](#snarkos-aot-auth-program)
* [`snarkos-aot auth fee`↴](#snarkos-aot-auth-fee)
* [`snarkos-aot auth id`↴](#snarkos-aot-auth-id)
* [`snarkos-aot auth cost`↴](#snarkos-aot-auth-cost)
* [`snarkos-aot auth deploy`↴](#snarkos-aot-auth-deploy)
* [`snarkos-aot man`↴](#snarkos-aot-man)
* [`snarkos-aot md`↴](#snarkos-aot-md)

## `snarkos-aot`

**Usage:** `snarkos-aot [OPTIONS] <COMMAND>`

###### **Subcommands:**

* `genesis` — 
* `accounts` — 
* `ledger` — 
* `run` — 
* `auth` — 
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

* `-g`, `--genesis-key <GENESIS_KEY>` — The private key to use when generating the genesis block. Generates one randomly if not passed
* `-o`, `--output <OUTPUT>` — Where to write the genesis block to

  Default value: `genesis.block`
* `--committee-size <COMMITTEE_SIZE>` — The committee size. Not used if --bonded-balances is set

  Default value: `4`
* `--committee-output <COMMITTEE_OUTPUT>` — A place to optionally write out the generated committee private keys JSON
* `--additional-accounts <ADDITIONAL_ACCOUNTS>` — Additional number of accounts that aren't validators to add balances to

  Default value: `0`
* `--additional-accounts-balance <additional-accounts-balance>` — The balance to add to the number of accounts specified by additional-accounts

  Default value: `100000000`
* `--additional-accounts-record-balance <ADDITIONAL_ACCOUNTS_RECORD_BALANCE>` — If --additional-accounts is passed you can additionally add an amount to give them in a record
* `--additional-accounts-output <ADDITIONAL_ACCOUNTS_OUTPUT>` — A place to write out the additionally generated accounts by --additional-accounts
* `--seed <SEED>` — The seed to use when generating committee private keys and the genesis block. If unpassed, uses DEVELOPMENT_MODE_RNG_SEED (1234567890u64)
* `--bonded-balance <BONDED_BALANCE>` — The bonded balance each bonded address receives. Not used if `--bonded-balances` is passed

  Default value: `10000000000000`
* `--bonded-balances <BONDED_BALANCES>` — An optional map from address to bonded balance. Overrides `--bonded-balance` and `--committee-size`
* `--bonded-withdrawal <BONDED_WITHDRAWAL>` — An optional to specify withdrawal addresses for the genesis committee
* `--bonded-commission <BONDED_COMMISSION>` — The bonded commission each bonded address uses. Not used if `--bonded-commissions` is passed. Defaults to 0. Must be 100 or less

  Default value: `0`
* `--bonded-commissions <BONDED_COMMISSIONS>` — An optional map from address to bonded commission. Overrides `--bonded-commission`. Defaults to 0. Must be 100 or less
* `--ledger <LEDGER>` — Optionally initialize a ledger as well



## `snarkos-aot accounts`

**Usage:** `snarkos-aot accounts [OPTIONS] [COUNT]`

###### **Arguments:**

* `<COUNT>` — Number of accounts to generate

  Default value: `1`

###### **Options:**

* `-v`, `--vanity <VANITY>` — Vanity prefix for addresses
* `-o`, `--output <OUTPUT>` — Where to write the output to
* `-s`, `--seed <seed>` — The seed to use when generating private keys If unpassed or used with --vanity, uses a random seed



## `snarkos-aot ledger`

**Usage:** `snarkos-aot ledger [OPTIONS] --ledger <LEDGER> <COMMAND>`

###### **Subcommands:**

* `init` — 
* `view` — 
* `rewind` — 
* `replay` — 
* `execute` — 
* `query` — Receive inquiries on /<network>/latest/stateRoot
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

**Usage:** `snarkos-aot ledger execute [OPTIONS] --query <QUERY> [JSON]`

###### **Arguments:**

* `<JSON>` — Authorization flags as json

###### **Options:**

* `-e`, `--exec-mode <EXEC_MODE>`

  Default value: `local`

  Possible values: `local`, `remote`

* `-q`, `--query <QUERY>` — Query endpoint
* `-b`, `--broadcast` — The authorization for the fee execution. Whether to broadcast the transaction

  Default value: `false`

  Possible values: `true`, `false`

* `-a`, `--auth <AUTH>` — Authorization of the program function
* `-f`, `--fee-auth <FEE_AUTH>`
* `-o`, `--owner <OWNER>`
* `-d`, `--deployment <DEPLOYMENT>`



## `snarkos-aot ledger query`

Receive inquiries on /<network>/latest/stateRoot

**Usage:** `snarkos-aot ledger query [OPTIONS]`

###### **Options:**

* `--port <PORT>` — Port to listen on for incoming messages

  Default value: `3030`
* `--bind <BIND>`

  Default value: `0.0.0.0`
* `--readonly` — When true, the POST /block endpoint will not be available

  Possible values: `true`, `false`

* `--record` — Receive messages from /<network>/transaction/broadcast and record them to the output

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

**Usage:** `snarkos-aot run [OPTIONS] --ledger <LEDGER> --type <type> <--private-key <PRIVATE_KEY>|--private-key-file <PRIVATE_KEY_FILE>>`

###### **Options:**

* `-g`, `--genesis <GENESIS>` — A path to the genesis block to initialize the ledger from
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
* `--validators <VALIDATORS>` — Specify the IP address and port of the validator(s) to connect to
* `--rest-rps <REST_RPS>` — Specify the requests per second (RPS) rate limit per IP for the REST server

  Default value: `1000`
* `--retention-policy <RETENTION_POLICY>`
* `--agent-status-port <AGENT_STATUS_PORT>` — When present, emits the agent status on the given port



## `snarkos-aot auth`

**Usage:** `snarkos-aot auth <COMMAND>`

###### **Subcommands:**

* `execute` — Execute an authorization
* `program` — Authorize a program execution
* `fee` — Authorize the fee for a program execution
* `id` — Given an authorization (and fee), return the transaction ID
* `cost` — Estimate the cost of a program execution or deployment
* `deploy` — Deploy a program to the network



## `snarkos-aot auth execute`

Execute an authorization

**Usage:** `snarkos-aot auth execute [OPTIONS] --query <QUERY> [JSON]`

###### **Arguments:**

* `<JSON>` — Authorization flags as json

###### **Options:**

* `-e`, `--exec-mode <EXEC_MODE>`

  Default value: `local`

  Possible values: `local`, `remote`

* `-q`, `--query <QUERY>` — Query endpoint
* `-b`, `--broadcast` — The authorization for the fee execution. Whether to broadcast the transaction

  Default value: `false`

  Possible values: `true`, `false`

* `-a`, `--auth <AUTH>` — Authorization of the program function
* `-f`, `--fee-auth <FEE_AUTH>`
* `-o`, `--owner <OWNER>`
* `-d`, `--deployment <DEPLOYMENT>`



## `snarkos-aot auth program`

Authorize a program execution

**Usage:** `snarkos-aot auth program [OPTIONS] <--private-key <PRIVATE_KEY>|--private-key-file <PRIVATE_KEY_FILE>> <LOCATOR> [INPUTS]...`

###### **Arguments:**

* `<LOCATOR>` — Program ID and function name (eg. credits.aleo/transfer_public)
* `<INPUTS>` — Program inputs (eg. 1u64 5field)

###### **Options:**

* `--private-key <PRIVATE_KEY>` — Specify the account private key of the node
* `--private-key-file <PRIVATE_KEY_FILE>` — Specify the account private key of the node
* `--fee-private-key <FEE_PRIVATE_KEY>` — Specify the account private key of the node
* `--fee-private-key-file <FEE_PRIVATE_KEY_FILE>` — Specify the account private key of the node
* `--skip-fee` — Prevent the fee from being included in the authorization

  Possible values: `true`, `false`

* `--priority-fee <PRIORITY_FEE>` — The priority fee in microcredits

  Default value: `0`
* `--record <RECORD>` — The record for a private fee
* `-q`, `--query <QUERY>`



## `snarkos-aot auth fee`

Authorize the fee for a program execution

**Usage:** `snarkos-aot auth fee [OPTIONS] <--private-key <PRIVATE_KEY>|--private-key-file <PRIVATE_KEY_FILE>>`

###### **Options:**

* `--private-key <PRIVATE_KEY>` — Specify the account private key of the node
* `--private-key-file <PRIVATE_KEY_FILE>` — Specify the account private key of the node
* `--priority-fee <PRIORITY_FEE>` — The priority fee in microcredits

  Default value: `0`
* `--record <RECORD>` — The record for a private fee
* `-a`, `--auth <AUTH>` — The Authorization for the program program execution
* `-d`, `--deployment <DEPLOYMENT>`
* `-i`, `--id <ID>` — The ID of the deployment or program execution
* `-c`, `--cost <COST>` — Estimated cost of the deployment or program execution



## `snarkos-aot auth id`

Given an authorization (and fee), return the transaction ID

**Usage:** `snarkos-aot auth id [OPTIONS] [JSON]`

###### **Arguments:**

* `<JSON>` — Authorization flags as json

###### **Options:**

* `-a`, `--auth <AUTH>` — Authorization of the program function
* `-f`, `--fee-auth <FEE_AUTH>`
* `-o`, `--owner <OWNER>`
* `-d`, `--deployment <DEPLOYMENT>`



## `snarkos-aot auth cost`

Estimate the cost of a program execution or deployment

**Usage:** `snarkos-aot auth cost [OPTIONS] [JSON]`

###### **Arguments:**

* `<JSON>` — Authorization flags as json

###### **Options:**

* `--query <QUERY>`
* `-a`, `--auth <AUTH>` — Authorization of the program function
* `-f`, `--fee-auth <FEE_AUTH>`
* `-o`, `--owner <OWNER>`
* `-d`, `--deployment <DEPLOYMENT>`



## `snarkos-aot auth deploy`

Deploy a program to the network

**Usage:** `snarkos-aot auth deploy [OPTIONS] <--private-key <PRIVATE_KEY>|--private-key-file <PRIVATE_KEY_FILE>> <PROGRAM>`

###### **Arguments:**

* `<PROGRAM>`

###### **Options:**

* `--private-key <PRIVATE_KEY>` — Specify the account private key of the node
* `--private-key-file <PRIVATE_KEY_FILE>` — Specify the account private key of the node
* `--fee-private-key <FEE_PRIVATE_KEY>` — Specify the account private key of the node
* `--fee-private-key-file <FEE_PRIVATE_KEY_FILE>` — Specify the account private key of the node
* `--skip-fee` — Prevent the fee from being included in the authorization

  Possible values: `true`, `false`

* `--priority-fee <PRIORITY_FEE>` — The priority fee in microcredits

  Default value: `0`
* `--record <RECORD>` — The record for a private fee
* `-q`, `--query <QUERY>`



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
