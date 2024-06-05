# Command-Line Help for `snops`

This document contains the help content for the `snops` command-line program.

**Command Overview:**

* [`snops`↴](#snops)
* [`snops man`↴](#snops-man)
* [`snops md`↴](#snops-md)

## `snops`

**Usage:** `snops [OPTIONS] <COMMAND>`

###### **Subcommands:**

* `man` — For generating cli manpages. Only with the mangen feature enabled
* `md` — For generating cli markdown. Only with the clipages feature enabled

###### **Options:**

* `--bind <BIND_ADDR>`

  Default value: `0.0.0.0`
* `--port <PORT>` — Control plane server port

  Default value: `1234`
* `--prometheus <PROMETHEUS>` — Optional URL referencing a Prometheus server
* `--loki <LOKI>` — Optional URL referencing a Loki server
* `--prometheus-location <PROMETHEUS_LOCATION>`

  Default value: `docker`
* `--path <PATH>` — Path to the directory containing the stored data

  Default value: `snops-control-data`
* `--hostname <HOSTNAME>` — Hostname to advertise to the control plane, used when resolving the control plane's address for external cannons can be an external IP or FQDN, will have the port appended



## `snops man`

For generating cli manpages. Only with the mangen feature enabled

**Usage:** `snops man [DIRECTORY]`

###### **Arguments:**

* `<DIRECTORY>`

  Default value: `target/man/snops-cli`



## `snops md`

For generating cli markdown. Only with the clipages feature enabled

**Usage:** `snops md [DIRECTORY]`

###### **Arguments:**

* `<DIRECTORY>`

  Default value: `snops_book/user_guide/clis`



<hr/>

<small><i>
    This document was generated automatically by
    <a href="https://crates.io/crates/clap-markdown"><code>clap-markdown</code></a>.
</i></small>
