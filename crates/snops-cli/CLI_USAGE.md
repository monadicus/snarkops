# Command-Line Help for `snops-cli`

This document contains the help content for the `snops-cli` command-line program.

**Command Overview:**

* [`snops-cli`↴](#snops-cli)
* [`snops-cli autocomplete`↴](#snops-cli-autocomplete)
* [`snops-cli agent`↴](#snops-cli-agent)
* [`snops-cli agent info`↴](#snops-cli-agent-info)
* [`snops-cli agent list`↴](#snops-cli-agent-list)
* [`snops-cli agent tps`↴](#snops-cli-agent-tps)
* [`snops-cli env`↴](#snops-cli-env)
* [`snops-cli env agent`↴](#snops-cli-env-agent)
* [`snops-cli env agents`↴](#snops-cli-env-agents)
* [`snops-cli env clean`↴](#snops-cli-env-clean)
* [`snops-cli env list`↴](#snops-cli-env-list)
* [`snops-cli env timeline`↴](#snops-cli-env-timeline)
* [`snops-cli env timeline apply`↴](#snops-cli-env-timeline-apply)
* [`snops-cli env timeline delete`↴](#snops-cli-env-timeline-delete)
* [`snops-cli env timeline info`↴](#snops-cli-env-timeline-info)
* [`snops-cli env timeline list`↴](#snops-cli-env-timeline-list)
* [`snops-cli env topology`↴](#snops-cli-env-topology)
* [`snops-cli env topology-resolved`↴](#snops-cli-env-topology-resolved)
* [`snops-cli env prepare`↴](#snops-cli-env-prepare)
* [`snops-cli env storage`↴](#snops-cli-env-storage)
* [`snops-cli env start`↴](#snops-cli-env-start)
* [`snops-cli env stop`↴](#snops-cli-env-stop)

## `snops-cli`

**Usage:** `snops-cli [OPTIONS] <COMMAND>`

###### **Subcommands:**

* `autocomplete` — Generate shell completions
* `agent` — For interacting with snop environments
* `env` — For interacting with snop environments

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

For interacting with snop environments

**Usage:** `snops-cli agent [ID] <COMMAND>`

###### **Subcommands:**

* `info` — Get the specific agent
* `list` — List all agents. Ignores the agent id
* `tps` — Get the specific agent's TPS

###### **Arguments:**

* `<ID>` — Show a specific agent's info

  Default value: `dummy_value___`



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

* `agent` — Get an env's specific agent by
* `agents` — List an env's agents
* `clean` — Clean a specific environment
* `list` — List all environments. Ignores the env id
* `timeline` — List all steps for a specific timeline
* `topology` — Show the current topology of a specific environment
* `topology-resolved` — Show the resolved topology of a specific environment. Shows only internal agents
* `prepare` — Prepare a (test) environment
* `storage` — Get an env's storage info
* `start` — Start an environment's timeline (a test)
* `stop` — Stop an environment's timeline

###### **Arguments:**

* `<ID>` — Show a specific env

  Default value: `default`



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



## `snops-cli env timeline`

List all steps for a specific timeline

**Usage:** `snops-cli env timeline [ID] <COMMAND>`

###### **Subcommands:**

* `apply` — Apply a timeline to an environment
* `delete` — Delete a timeline from an environment.zs
* `info` — List all steps for a specific timeline
* `list` — List all timelines for a specific environment. Timeline id is ignored

###### **Arguments:**

* `<ID>` — The timeline id

  Default value: `dummy_value___`



## `snops-cli env timeline apply`

Apply a timeline to an environment

**Usage:** `snops-cli env timeline apply`



## `snops-cli env timeline delete`

Delete a timeline from an environment.zs

**Usage:** `snops-cli env timeline delete`



## `snops-cli env timeline info`

List all steps for a specific timeline

**Usage:** `snops-cli env timeline info`



## `snops-cli env timeline list`

List all timelines for a specific environment. Timeline id is ignored

**Usage:** `snops-cli env timeline list`



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



## `snops-cli env start`

Start an environment's timeline (a test)

**Usage:** `snops-cli env start <TIMELINE_ID>`

###### **Arguments:**

* `<TIMELINE_ID>` — Start a specific timeline



## `snops-cli env stop`

Stop an environment's timeline

**Usage:** `snops-cli env stop <TIMELINE_ID>`

###### **Arguments:**

* `<TIMELINE_ID>` — Stop a specific timeline



<hr/>

<small><i>
    This document was generated automatically by
    <a href="https://crates.io/crates/clap-markdown"><code>clap-markdown</code></a>.
</i></small>
