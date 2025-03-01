# Command-Line Help for `snops-cli`

This document contains the help content for the `snops-cli` command-line program.

**Command Overview:**

* [`snops-cli`↴](#snops-cli)
* [`snops-cli autocomplete`↴](#snops-cli-autocomplete)
* [`snops-cli agent`↴](#snops-cli-agent)
* [`snops-cli agent find`↴](#snops-cli-agent-find)
* [`snops-cli agent info`↴](#snops-cli-agent-info)
* [`snops-cli agent kill`↴](#snops-cli-agent-kill)
* [`snops-cli agent list`↴](#snops-cli-agent-list)
* [`snops-cli agent tps`↴](#snops-cli-agent-tps)
* [`snops-cli agent status`↴](#snops-cli-agent-status)
* [`snops-cli agent set-log-level`↴](#snops-cli-agent-set-log-level)
* [`snops-cli agent set-snarkos-log-level`↴](#snops-cli-agent-set-snarkos-log-level)
* [`snops-cli env`↴](#snops-cli-env)
* [`snops-cli env action`↴](#snops-cli-env-action)
* [`snops-cli env action offline`↴](#snops-cli-env-action-offline)
* [`snops-cli env action online`↴](#snops-cli-env-action-online)
* [`snops-cli env action reboot`↴](#snops-cli-env-action-reboot)
* [`snops-cli env action execute`↴](#snops-cli-env-action-execute)
* [`snops-cli env action deploy`↴](#snops-cli-env-action-deploy)
* [`snops-cli env action config`↴](#snops-cli-env-action-config)
* [`snops-cli env agent`↴](#snops-cli-env-agent)
* [`snops-cli env agents`↴](#snops-cli-env-agents)
* [`snops-cli env auth`↴](#snops-cli-env-auth)
* [`snops-cli env balance`↴](#snops-cli-env-balance)
* [`snops-cli env block`↴](#snops-cli-env-block)
* [`snops-cli env height`↴](#snops-cli-env-height)
* [`snops-cli env transaction`↴](#snops-cli-env-transaction)
* [`snops-cli env transaction-details`↴](#snops-cli-env-transaction-details)
* [`snops-cli env delete`↴](#snops-cli-env-delete)
* [`snops-cli env info`↴](#snops-cli-env-info)
* [`snops-cli env list`↴](#snops-cli-env-list)
* [`snops-cli env topology`↴](#snops-cli-env-topology)
* [`snops-cli env topology-resolved`↴](#snops-cli-env-topology-resolved)
* [`snops-cli env apply`↴](#snops-cli-env-apply)
* [`snops-cli env mapping`↴](#snops-cli-env-mapping)
* [`snops-cli env mappings`↴](#snops-cli-env-mappings)
* [`snops-cli env program`↴](#snops-cli-env-program)
* [`snops-cli env storage`↴](#snops-cli-env-storage)
* [`snops-cli spec`↴](#snops-cli-spec)
* [`snops-cli spec node-keys`↴](#snops-cli-spec-node-keys)
* [`snops-cli spec nodes`↴](#snops-cli-spec-nodes)
* [`snops-cli spec num-agents`↴](#snops-cli-spec-num-agents)
* [`snops-cli spec network`↴](#snops-cli-spec-network)
* [`snops-cli spec check`↴](#snops-cli-spec-check)
* [`snops-cli set-log-level`↴](#snops-cli-set-log-level)
* [`snops-cli events`↴](#snops-cli-events)
* [`snops-cli man`↴](#snops-cli-man)
* [`snops-cli md`↴](#snops-cli-md)

## `snops-cli`

**Usage:** `snops-cli [OPTIONS] <COMMAND>`

###### **Subcommands:**

* `autocomplete` — Generate shell completions
* `agent` — For interacting with snop agents
* `env` — For interacting with snop environments
* `spec` — 
* `set-log-level` — 
* `events` — Listen to events from the control plane, optionally filtered
* `man` — For generating cli manpages. Only with the mangen feature enabled
* `md` — For generating cli markdown. Only with the clipages feature enabled

###### **Options:**

* `-u`, `--url <URL>` — The url the control plane is on

  Default value: `http://localhost:1234`



## `snops-cli autocomplete`

Generate shell completions

**Usage:** `snops-cli autocomplete <SHELL>`

###### **Arguments:**

* `<SHELL>` — Which shell you want to generate completions for

  Possible values: `bash`, `elvish`, `fish`, `powershell`, `zsh`




## `snops-cli agent`

For interacting with snop agents

**Usage:** `snops-cli agent [ID] <COMMAND>`

###### **Subcommands:**

* `find` — Find agents by set criteria. If all of client/compute/prover/validator are not specified it can be any one of them
* `info` — Get the specific agent
* `kill` — Kill the specific agent
* `list` — List all agents. Ignores the agent id
* `tps` — Get the specific agent's TPS
* `status` — Get the specific agent's status
* `set-log-level` — Set the log level of the agent
* `set-snarkos-log-level` — Set the log level of the node running on an agent

###### **Arguments:**

* `<ID>` — Show a specific agent's info

  Default value: `dummy_value___`



## `snops-cli agent find`

Find agents by set criteria. If all of client/compute/prover/validator are not specified it can be any one of them

**Usage:** `snops-cli agent find [OPTIONS]`

###### **Options:**

* `--client` — Whether the agent can be a client
* `--compute` — Whether the agent can be a compute
* `--prover` — Whether the agent can be a prover
* `--validator` — Whether the agent can be a validator
* `--env <ENV>` — Which env you are finding the agens from. Not specifing a env, means only inventoried agents are found
* `--all` — Means regardless of connection status, and state we find them
* `--labels <LABELS>` — The labels an agent should have
* `--local-pk` — If the agent has a local private key or not
* `--include-offline` — Whether to include offline agents as well



## `snops-cli agent info`

Get the specific agent

**Usage:** `snops-cli agent info`



## `snops-cli agent kill`

Kill the specific agent

**Usage:** `snops-cli agent kill`



## `snops-cli agent list`

List all agents. Ignores the agent id

**Usage:** `snops-cli agent list`



## `snops-cli agent tps`

Get the specific agent's TPS

**Usage:** `snops-cli agent tps`



## `snops-cli agent status`

Get the specific agent's status

**Usage:** `snops-cli agent status`



## `snops-cli agent set-log-level`

Set the log level of the agent

**Usage:** `snops-cli agent set-log-level <LEVEL>`

###### **Arguments:**

* `<LEVEL>` — The log level to set



## `snops-cli agent set-snarkos-log-level`

Set the log level of the node running on an agent

**Usage:** `snops-cli agent set-snarkos-log-level <VERBOSITY>`

###### **Arguments:**

* `<VERBOSITY>` — The log verbosity to set



## `snops-cli env`

For interacting with snop environments

**Usage:** `snops-cli env [ID] <COMMAND>`

###### **Subcommands:**

* `action` — Run an action on an environment
* `agent` — Get an env's specific agent by
* `agents` — List an env's agents
* `auth` — 
* `balance` — Lookup an account's balance
* `block` — Lookup a block or get the latest block
* `height` — Get the latest height from all agents in the env
* `transaction` — Lookup a transaction's block by a transaction id
* `transaction-details` — Lookup a transaction's details by a transaction id
* `delete` — Delete a specific environment
* `info` — Get an env's latest block/state root info
* `list` — List all environments. Ignores the env id
* `topology` — Show the current topology of a specific environment
* `topology-resolved` — Show the resolved topology of a specific environment. Shows only internal agents
* `apply` — Apply an environment spec
* `mapping` — Lookup a mapping by program id and mapping name
* `mappings` — Lookup a program's mappings only
* `program` — Lookup a program by its id
* `storage` — Get an env's storage info

###### **Arguments:**

* `<ID>` — Work with a specific env

  Default value: `default`



## `snops-cli env action`

Run an action on an environment

**Usage:** `snops-cli env action <COMMAND>`

###### **Subcommands:**

* `offline` — Turn the specified agents(and nodes) offline
* `online` — Turn the specified agents(and nodes) online
* `reboot` — Reboot the specified agents(and nodes)
* `execute` — Execute an aleo program function on the environment. i.e. credits.aleo/transfer_public
* `deploy` — Deploy an aleo program to the environment
* `config` — Configure the state of the target nodes



## `snops-cli env action offline`

Turn the specified agents(and nodes) offline

**Usage:** `snops-cli env action offline [OPTIONS] [NODES]...`

###### **Arguments:**

* `<NODES>` — The nodes to take offline. (eg. `validator/any`)

###### **Options:**

* `--async` — When present, don't wait for reconciles to finish before returning



## `snops-cli env action online`

Turn the specified agents(and nodes) online

**Usage:** `snops-cli env action online [OPTIONS] [NODES]...`

###### **Arguments:**

* `<NODES>` — The nodes to turn online (eg. `validator/any`)

###### **Options:**

* `--async` — When present, don't wait for reconciles to finish before returning



## `snops-cli env action reboot`

Reboot the specified agents(and nodes)

**Usage:** `snops-cli env action reboot [OPTIONS] [NODES]...`

###### **Arguments:**

* `<NODES>` — The nodes to reboot (eg. `validator/any`)

###### **Options:**

* `--async` — When present, don't wait for reconciles to finish before returning



## `snops-cli env action execute`

Execute an aleo program function on the environment. i.e. credits.aleo/transfer_public

**Usage:** `snops-cli env action execute [OPTIONS] <LOCATOR> [INPUTS]...`

###### **Arguments:**

* `<LOCATOR>` — `transfer_public` OR `credits.aleo/transfer_public`
* `<INPUTS>` — list of program inputs

###### **Options:**

* `--private-key <PRIVATE_KEY>` — Private key to use, can be `committee.0` to use committee member 0's key
* `--fee-private-key <FEE_PRIVATE_KEY>` — Private key to use for the fee. Defaults to the same as --private-key
* `-c`, `--cannon <CANNON>` — Desired cannon to fire the transaction
* `--priority-fee <PRIORITY_FEE>` — The optional priority fee to use
* `--fee-record <FEE_RECORD>` — The fee record to use if you want to pay the fee privately
* `--async` — When present, don't wait for transaction execution before returning



## `snops-cli env action deploy`

Deploy an aleo program to the environment

**Usage:** `snops-cli env action deploy [OPTIONS] <PROGRAM>`

###### **Arguments:**

* `<PROGRAM>` — Path to program or program content in stdin

###### **Options:**

* `-p`, `--private-key <PRIVATE_KEY>` — Private key to use, can be `committee.0` to use committee member 0's key
* `--fee-private-key <FEE_PRIVATE_KEY>` — Private key to use for the fee. Defaults to the same as --private-key
* `-c`, `--cannon <CANNON>` — Desired cannon to fire the transaction
* `--priority-fee <PRIORITY_FEE>` — The optional priority fee to use
* `--fee-record <FEE_RECORD>` — The fee record to use if you want to pay the fee privately
* `--async` — When present, don't wait for transaction execution before returning



## `snops-cli env action config`

Configure the state of the target nodes

**Usage:** `snops-cli env action config [OPTIONS] [NODES]...`

###### **Arguments:**

* `<NODES>` — The nodes to configure. (eg. `validator/any`)

###### **Options:**

* `-o`, `--online <ONLINE>` — Configure the online state of the target nodes

  Possible values: `true`, `false`

* `--height <HEIGHT>` — Configure the height of the target nodes
* `-p`, `--peers <PEERS>` — Configure the peers of the target nodes, or `none`
* `-v`, `--validators <VALIDATORS>` — Configure the validators of the target nodes, or `none`
* `-e`, `--env <ENV>` — Set environment variables for a node: `--env FOO=bar`
* `-d`, `--del-env <DEL_ENV>`
* `-b`, `--binary <BINARY>` — Configure the binary for a node
* `--private-key <PRIVATE_KEY>` — Configure the private key for a node
* `--async`



## `snops-cli env agent`

Get an env's specific agent by

**Usage:** `snops-cli env agent <KEY>`

###### **Arguments:**

* `<KEY>` — The agent's key. i.e validator/0, client/foo, prover/9, or combination



## `snops-cli env agents`

List an env's agents

**Usage:** `snops-cli env agents`



## `snops-cli env auth`

**Usage:** `snops-cli env auth [OPTIONS] <AUTH>`

###### **Arguments:**

* `<AUTH>` — Authorization to execute and broadcast

###### **Options:**

* `--async` — When present, don't wait for transaction execution before returning
* `-c`, `--cannon <CANNON>` — Desired cannon to fire the transaction

  Default value: `default`



## `snops-cli env balance`

Lookup an account's balance

**Usage:** `snops-cli env balance <ADDRESS>`

###### **Arguments:**

* `<ADDRESS>` — Address to lookup balance for



## `snops-cli env block`

Lookup a block or get the latest block

**Usage:** `snops-cli env block [HEIGHT_OR_HASH]`

###### **Arguments:**

* `<HEIGHT_OR_HASH>` — The block's height or hash

  Default value: `latest`



## `snops-cli env height`

Get the latest height from all agents in the env

**Usage:** `snops-cli env height`



## `snops-cli env transaction`

Lookup a transaction's block by a transaction id

**Usage:** `snops-cli env transaction <ID>`

###### **Arguments:**

* `<ID>`



## `snops-cli env transaction-details`

Lookup a transaction's details by a transaction id

**Usage:** `snops-cli env transaction-details <ID>`

###### **Arguments:**

* `<ID>`



## `snops-cli env delete`

Delete a specific environment

**Usage:** `snops-cli env delete`



## `snops-cli env info`

Get an env's latest block/state root info

**Usage:** `snops-cli env info`



## `snops-cli env list`

List all environments. Ignores the env id

**Usage:** `snops-cli env list`



## `snops-cli env topology`

Show the current topology of a specific environment

**Usage:** `snops-cli env topology`



## `snops-cli env topology-resolved`

Show the resolved topology of a specific environment. Shows only internal agents

**Usage:** `snops-cli env topology-resolved`



## `snops-cli env apply`

Apply an environment spec

**Usage:** `snops-cli env apply [OPTIONS] <SPEC>`

###### **Arguments:**

* `<SPEC>` — The environment spec file

###### **Options:**

* `--async` — When present, don't wait for reconciles to finish before returning



## `snops-cli env mapping`

Lookup a mapping by program id and mapping name

**Usage:** `snops-cli env mapping <PROGRAM> <MAPPING> <KEY>`

###### **Arguments:**

* `<PROGRAM>` — The program name
* `<MAPPING>` — The mapping name
* `<KEY>` — The key to lookup



## `snops-cli env mappings`

Lookup a program's mappings only

**Usage:** `snops-cli env mappings <PROGRAM>`

###### **Arguments:**

* `<PROGRAM>` — The program name



## `snops-cli env program`

Lookup a program by its id

**Usage:** `snops-cli env program <ID>`

###### **Arguments:**

* `<ID>`



## `snops-cli env storage`

Get an env's storage info

**Usage:** `snops-cli env storage`



## `snops-cli spec`

**Usage:** `snops-cli spec <COMMAND>`

###### **Subcommands:**

* `node-keys` — Extract all node keys from a spec file
* `nodes` — Extract all nodes from a spec file
* `num-agents` — Count how many agents would be needed to run the spec
* `network` — Get the network id a spec
* `check` — Check the spec for errors



## `snops-cli spec node-keys`

Extract all node keys from a spec file

**Usage:** `snops-cli spec node-keys [OPTIONS] <SPEC>`

###### **Arguments:**

* `<SPEC>` — The environment spec file

###### **Options:**

* `--external` — When present, include external keys



## `snops-cli spec nodes`

Extract all nodes from a spec file

**Usage:** `snops-cli spec nodes <SPEC>`

###### **Arguments:**

* `<SPEC>` — The environment spec file



## `snops-cli spec num-agents`

Count how many agents would be needed to run the spec

**Usage:** `snops-cli spec num-agents <SPEC>`

###### **Arguments:**

* `<SPEC>` — The environment spec file



## `snops-cli spec network`

Get the network id a spec

**Usage:** `snops-cli spec network <SPEC>`

###### **Arguments:**

* `<SPEC>` — The environment spec file



## `snops-cli spec check`

Check the spec for errors

**Usage:** `snops-cli spec check <SPEC>`

###### **Arguments:**

* `<SPEC>` — The environment spec file



## `snops-cli set-log-level`

**Usage:** `snops-cli set-log-level <LEVEL>`

###### **Arguments:**

* `<LEVEL>`



## `snops-cli events`

Listen to events from the control plane, optionally filtered

**Usage:** `snops-cli events [FILTER]`

###### **Arguments:**

* `<FILTER>` — The event filter to apply, such as `agent-connected` or `all-of(env-is(default),node-target-is(validator/any))`

  Default value: `unfiltered`



## `snops-cli man`

For generating cli manpages. Only with the mangen feature enabled

**Usage:** `snops-cli man [DIRECTORY]`

###### **Arguments:**

* `<DIRECTORY>` — Directory to write manpages to

  Default value: `target/man/snops-cli`



## `snops-cli md`

For generating cli markdown. Only with the clipages feature enabled

**Usage:** `snops-cli md [DIRECTORY]`

###### **Arguments:**

* `<DIRECTORY>` — Directory to write markdown to

  Default value: `snops_book/user_guide/clis`



<hr/>

<small><i>
    This document was generated automatically by
    <a href="https://crates.io/crates/clap-markdown"><code>clap-markdown</code></a>.
</i></small>
