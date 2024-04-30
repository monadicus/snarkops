<!-- TODO snops image <p align="center">
  <a href="https://monadic.us/">
    <img width="90%" alt="snops" src="">
  </a>
</p> -->

<h1 align="center">
  SNOPS
</h1>

This repository is home to the `snops` (snarkOS operations) ecosystem, and
`snarkos-aot`, a crate for performing ahead-of-time ledger actions.

snops is a suite of tools that can be used to maintain [Aleo](https://aleo.org/)
network environments. The environments are defined in a manner similar to
infrastructure-as-code, which means you can create repeatable infrastructure
using the environment schema.

This can be used to create devnets, simulate events on the network like outages
and attacks, and guarantee metrics.

## Architecture

A snops "instance" is composed of a control plane and any number of agents:

- **Control plane:** a machine that is running the `snops` crate. Responsible
  for the deployment of snarkOS nodes on agents.
- **Agents:** machines that run the `snops-agent` crate. Communicates with the
  control plane to receive state reconciliations and other messages.

In the default state, the control plane holds a pool of available agents that
have connected to it. An agent is in inventory mode if it is not currently
running a snarkOS node. It can be turned into a node by preparing an
**environment**, in which the control plane will delegate agents in inventory to
become real nodes that match the environment's topology.

### Environments

An environment is a collection of snops documents that describe a particular use
case's storage, topology, timeline, and expected outcomes (see some
[examples](./specs/)).

<!-- TODO: more document types -->

- **Storage:** describes the ledger that agents assigned to this environment
  will use. Can be an existing ledger or a generated one.
- **Topology:** describes the nodes that must be present for this environment to
  be fully prepared. If a control plane does not have enough agents asked for by
  an environment's topology, the control plane will refuse to prepare the
  environment.
- **Timeline:** an optional description of the _events_ that will be simulated.
  Used to trigger actions like intentional outages, ledger manipulations, config
  changes, and more. This is the primary vector by which snops can be used as a
  testing platform.
- **Outcomes:** optional descrption of expected outcomes after the execution of
  a timeline. For some tests, such as guaranteeing a particular TPS after some
  events have been simulated, it is useful to objectively verify whether or not
  a test succeeded. Outcomes are based on PromQL queries, and a Prometheus
  instance will be queried to check whether or not an environment's outcomes
  were properly met.

### Control plane

The control plane runs as a daemon. It orchestrates any agents that connect to
it, and listens for requests from implementors of its HTTP API (such as the
[snops-cli](./crates/snops-cli/)) for further instructions, like preparing an
environment.

For metrics and outcome guarantees, the control plane can also be linked to a
Prometheus server. The Prometheus server can be local or remote.

For logging, the control plane can be liked to a Loki instance.

The Prometheus metrics can also be hooked into a Grafana dashboard. The Loki instance can also be linked to a Grafana dashboard and visuzlized there.

For an example deployment of Prometheus, Loki, and Grafana you can refer to our example configurations in `scripts/metrics`.

<!-- TODO nice to have eventually -->
<!-- In order to instruct the control plane after it has been started, you can use
the included snops-cli. TODO: how? -->

### Agents

Similarly, agents can run as a daemon on separate machines.

## Installation and usage

snops requires a local clone of `snarkos` and `snarkvm` in the parent directory.
That is, your file tree should look like:

```
- <parent folder>
  - snarkos
  - snarkops
  - snarkvm
```

For each of the snops binaries, use `--help` for information on how to use its
CLI.

### Starting the control plane

To start the control plane, build and execute the `snops` crate binary or use

```bash
./scripts/control_plane.sh
```

### Starting agents

<!-- TODO: agent containerization -->

For local development, it is handy to use the agent script in the
[scripts](./scripts/) directory:

```bash
# start four (indexed) agents
./scripts/agent.sh 0
./scripts/agent.sh 1
./scripts/agent.sh 2
./scripts/agent.sh 3
```

### Preparing an environment

An environment can be prepared with `snops-cli`:

```bash
snops-cli env prepare my-env-spec.yaml
```

In a dev environment, you can use the following script:

```bash
./scripts/env_start.sh specs/test-4-validators.yaml
```
