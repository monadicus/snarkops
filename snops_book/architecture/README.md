# Architecture

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
