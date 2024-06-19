# Command-Line Help for `snops-agent`

This document contains the help content for the `snops-agent` command-line program.

**Command Overview:**

* [`snops-agent`↴](#snops-agent)
* [`snops-agent man`↴](#snops-agent-man)
* [`snops-agent md`↴](#snops-agent-md)

## `snops-agent`

**Usage:** `snops-agent [OPTIONS] <COMMAND>`

###### **Subcommands:**

* `man` — For generating cli manpages. Only with the mangen feature enabled
* `md` — For generating cli markdown. Only with the clipages feature enabled

###### **Options:**

* `--endpoint <ENDPOINT>` — Control plane endpoint address (IP, or wss://host, http://host)
* `--id <ID>`
* `--private-key-file <PRIVATE_KEY_FILE>` — Locally provided private key file, used for envs where private keys are locally provided
* `--labels <LABELS>`
* `--path <PATH>` — Path to the directory containing the stored data and configuration

  Default value: `./snops-data`
* `--external <EXTERNAL>` — Enable the agent to fetch its external address. Necessary to determine which agents are on shared networks, and for external-to-external connections
* `--internal <INTERNAL>` — Manually specify internal addresses
* `--bind <BIND_ADDR>`

  Default value: `0.0.0.0`
* `--node <NODE>` — Specify the IP address and port for the node server

  Default value: `4130`
* `--bft <BFT>` — Specify the IP address and port for the BFT

  Default value: `5000`
* `--rest <REST>` — Specify the IP address and port for the REST server

  Default value: `3030`
* `--metrics <METRICS>` — Specify the port for the metrics

  Default value: `9000`
* `--validator` — Enable running a validator node

  Possible values: `true`, `false`

* `--prover` — Enable running a prover node

  Possible values: `true`, `false`

* `--client` — Enable running a client node

  Possible values: `true`, `false`

* `--compute` — Enable functioning as a compute target when inventoried

  Possible values: `true`, `false`

* `-q`, `--quiet` — Run the agent in quiet mode, suppressing most node output

  Default value: `false`

  Possible values: `true`, `false`




## `snops-agent man`

For generating cli manpages. Only with the mangen feature enabled

**Usage:** `snops-agent man [DIRECTORY]`

###### **Arguments:**

* `<DIRECTORY>` — Directory to write manpages to

  Default value: `target/man/snops-cli`



## `snops-agent md`

For generating cli markdown. Only with the clipages feature enabled

**Usage:** `snops-agent md [DIRECTORY]`

###### **Arguments:**

* `<DIRECTORY>` — Directory to write markdown to

  Default value: `snops_book/user_guide/clis`



<hr/>

<small><i>
    This document was generated automatically by
    <a href="https://crates.io/crates/clap-markdown"><code>clap-markdown</code></a>.
</i></small>
