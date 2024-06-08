# Command-Line Help for `snops-cli`

This document contains the help content for the `snops-cli` command-line program.

**Command Overview:**

* [`snops-cli`↴](#snops-cli)
* [`snops-cli autocomplete`↴](#snops-cli-autocomplete)
* [`snops-cli agent`↴](#snops-cli-agent)
* [`snops-cli agent find`↴](#snops-cli-agent-find)
* [`snops-cli agent info`↴](#snops-cli-agent-info)
* [`snops-cli agent list`↴](#snops-cli-agent-list)
* [`snops-cli agent tps`↴](#snops-cli-agent-tps)
* [`snops-cli env`↴](#snops-cli-env)
* [`snops-cli env action`↴](#snops-cli-env-action)
* [`snops-cli env action offline`↴](#snops-cli-env-action-offline)
* [`snops-cli env action online`↴](#snops-cli-env-action-online)
* [`snops-cli env action reboot`↴](#snops-cli-env-action-reboot)
* [`snops-cli env action execute`↴](#snops-cli-env-action-execute)
* [`snops-cli env action config`↴](#snops-cli-env-action-config)
* [`snops-cli env agent`↴](#snops-cli-env-agent)
* [`snops-cli env agents`↴](#snops-cli-env-agents)
* [`snops-cli env clean`↴](#snops-cli-env-clean)
* [`snops-cli env list`↴](#snops-cli-env-list)
* [`snops-cli env topology`↴](#snops-cli-env-topology)
* [`snops-cli env topology-resolved`↴](#snops-cli-env-topology-resolved)
* [`snops-cli env prepare`↴](#snops-cli-env-prepare)
* [`snops-cli env storage`↴](#snops-cli-env-storage)
* [`snops-cli man`↴](#snops-cli-man)
* [`snops-cli md`↴](#snops-cli-md)

## `snops-cli`

**Usage:** `snops-cli [OPTIONS] <COMMAND>`

###### **Subcommands:**

* `autocomplete` — Generate shell completions
* `agent` — For interacting with snop agents
* `env` — For interacting with snop environments
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
* `list` — List all agents. Ignores the agent id
* `tps` — Get the specific agent's TPS

###### **Arguments:**

* `<ID>` — Show a specific agent's info

  Default value: `dummy_value___`



## `snops-cli agent find`

Find agents by set criteria. If all of client/compute/prover/validator are not specified it can be any one of them

**Usage:** `snops-cli agent find [OPTIONS]`

###### **Options:**

* `--client` — Whether the agent can be a client

  Possible values: `true`, `false`

* `--compute` — Whether the agent can be a compute

  Possible values: `true`, `false`

* `--prover` — Whether the agent can be a prover

  Possible values: `true`, `false`

* `--validator` — Whether the agent can be a validator

  Possible values: `true`, `false`

* `--env <ENV>` — Which env you are finding the agens from. Not specifing a env, means only inventoried agents are found
* `--all` — Means regardless of connection status, and state we find them

  Possible values: `true`, `false`

* `--labels <LABELS>` — The labels an agent should have
* `--local-pk` — If the agent has a local private key or not

  Possible values: `true`, `false`

* `--include-offline` — Wether to include offline agents as well

  Possible values: `true`, `false`




## `snops-cli agent info`

Get the specific agent

**Usage:** `snops-cli agent info`



## `snops-cli agent list`

List all agents. Ignores the agent id

**Usage:** `snops-cli agent list`



## `snops-cli agent tps`

Get the specific agent's TPS

**Usage:** `snops-cli agent tps`



## `snops-cli env`

For interacting with snop environments

**Usage:** `snops-cli env [ID] <COMMAND>`

###### **Subcommands:**

* `action` — For interacting with snop environments
* `agent` — Get an env's specific agent by
* `agents` — List an env's agents
* `clean` — Clean a specific environment
* `list` — List all environments. Ignores the env id
* `topology` — Show the current topology of a specific environment
* `topology-resolved` — Show the resolved topology of a specific environment. Shows only internal agents
* `prepare` — Prepare a (test) environment
* `storage` — Get an env's storage info

###### **Arguments:**

* `<ID>` — Show a specific env

  Default value: `default`



## `snops-cli env action`

For interacting with snop environments

**Usage:** `snops-cli env action <COMMAND>`

###### **Subcommands:**

* `offline` — 
* `online` — 
* `reboot` — 
* `execute` — 
* `config` — 



## `snops-cli env action offline`

**Usage:** `snops-cli env action offline [NODES]...`

###### **Arguments:**

* `<NODES>`



## `snops-cli env action online`

**Usage:** `snops-cli env action online [NODES]...`

###### **Arguments:**

* `<NODES>`



## `snops-cli env action reboot`

**Usage:** `snops-cli env action reboot [NODES]...`

###### **Arguments:**

* `<NODES>`



## `snops-cli env action execute`

**Usage:** `snops-cli env action execute [OPTIONS] <LOCATOR> [INPUTS]...`

###### **Arguments:**

* `<LOCATOR>` — `transfer_public` OR `credits.aleo/transfer_public`
* `<INPUTS>` — list of program inputs

###### **Options:**

* `-p`, `--private-key <PRIVATE_KEY>` — Private key to use, can be `committee.0` to use committee member 0's key
* `-c`, `--cannon <CANNON>` — Desired cannon to fire the transaction
* `--priority-fee <PRIORITY_FEE>`
* `-f`, `--fee-record <FEE_RECORD>`



## `snops-cli env action config`

**Usage:** `snops-cli env action config [OPTIONS] [NODES]...`

###### **Arguments:**

* `<NODES>` — The nodes to configure

###### **Options:**

* `-o`, `--online <ONLINE>` — Configure the online state of the target nodes

  Possible values: `true`, `false`

* `--height <HEIGHT>` — Configure the height of the target nodes
* `-p`, `--peers <PEERS>` — Configure the peers of the target nodes, or `none`
* `-v`, `--validators <VALIDATORS>` — Configure the validators of the target nodes, or `none`



## `snops-cli env agent`

Get an env's specific agent by

**Usage:** `snops-cli env agent <KEY>`

###### **Arguments:**

* `<KEY>` — The agent's key. i.e validator/0, client/foo, prover/9, or combination



## `snops-cli env agents`

List an env's agents

**Usage:** `snops-cli env agents`



## `snops-cli env clean`

Clean a specific environment

**Usage:** `snops-cli env clean`



## `snops-cli env list`

List all environments. Ignores the env id

**Usage:** `snops-cli env list`



## `snops-cli env topology`

Show the current topology of a specific environment

**Usage:** `snops-cli env topology`



## `snops-cli env topology-resolved`

Show the resolved topology of a specific environment. Shows only internal agents

**Usage:** `snops-cli env topology-resolved`



## `snops-cli env prepare`

Prepare a (test) environment

**Usage:** `snops-cli env prepare <SPEC>`

###### **Arguments:**

* `<SPEC>` — The test spec file



## `snops-cli env storage`

Get an env's storage info

**Usage:** `snops-cli env storage`



## `snops-cli man`

For generating cli manpages. Only with the mangen feature enabled

**Usage:** `snops-cli man [DIRECTORY]`

###### **Arguments:**

* `<DIRECTORY>`

  Default value: `target/man/snops-cli`



## `snops-cli md`

For generating cli markdown. Only with the clipages feature enabled

**Usage:** `snops-cli md [DIRECTORY]`

###### **Arguments:**

* `<DIRECTORY>`

  Default value: `snops_book/user_guide/clis`



<hr/>

<small><i>
    This document was generated automatically by
    <a href="https://crates.io/crates/clap-markdown"><code>clap-markdown</code></a>.
</i></small>
