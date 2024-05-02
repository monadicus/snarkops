# Control plane:

A machine that is running the `snops` crate.

The control plane runs as a daemon. It orchestrates any agents that connect to
it, and listens for requests from implementors of its HTTP API (such as the
[snops-cli](TODO)) for further instructions, like preparing an
environment.

## Responsibilites

The control plane has many responsibilites.

### Binary Distribution

The control plane will manage multiple binaries:

#### Agent Binary

The agent binary itself.

This allows the agents and the control plane to always be the same version.

#### Runner Binaries

The runner binaries. So an agent can ask for whatever version it needs from the control plane.

### Environments

An environment is a collection of snops documents that describe a particular use
case's storage, topology, timeline, and expected outcomes. You will learn more about Environments later in [Environments](../user_guide/envs/README.md).

#### Topology

The topology of agents and how they connect as validators, clients, provers, or transaction cannons. Additionally, says which storage is being used.

#### Storage Management

The control plane will manage the storage and generation of Ledgers for agents to ask for.

This can be:

- An existing ledger.
- A new ledger.
- Then genesis block of a running network, i.e. testnet.
- Ledger checkpoints.

#### Cannons

Cannons are transaction cannons.

Where for public transactions you can generate them Ahead-of-Time(AOT). However, you can also generate them live.

For private transactions only live mode is supported.

Currently, only calls to `credits.aleo` are supported.

#### Timelines

An optional description of the _events_ that will be simulated.
Used to trigger actions like intentional outages, ledger manipulations, config
changes, and more.

This is the primary vector by which snops can be used as a
testing platform.

You can apply more than one timeline to an environment.

#### Outcomes

An optional descrption of expected outcomes after the execution of a timeline. This does requires a Prometheus instance for [metrics](#metrics-and-logging) to be running.

For some tests, such as guaranteeing a particular TPS after some events have been simulated, it is useful to objectively verify whether or not a test succeeded.

Outcomes are based on PromQL queries, and a Prometheus instance will be queried to check whether or not an environment's outcomes
were properly met.

### Agent Delegation

In the default state, the control plane holds a pool of available agents that
have connected to it.

Agents have two States:

- _Inventoried_: An agent is in inventory mode if it is not currently running a snarkOS node.
- _Associated_: It becomes associated with an **environment** when one is prepared. As the control plane will delegate agents in inventory to the **environment**.

### Metrics and Logging

For metrics and outcome guarantees, the control plane can also be linked to a
Prometheus server. The Prometheus server can be local or remote.

For logging, the control plane can be liked to a Loki instance.

The Prometheus metrics can also be hooked into a Grafana dashboard. The Loki instance can also be linked to a Grafana dashboard and visuzlized there.

For an example deployment of Prometheus, Loki, and Grafana you can refer to our example configurations in `scripts/metrics`.
