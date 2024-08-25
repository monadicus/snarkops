# Agent

The binary has several options you can run mess with at launch and some are required that we will go over here.

However, for a more in depth information you can read about the CLI options [here](../clis/SNOPS_AGENT.md).

## Running the Agent

The `agent` can be run on the same machine as the control plane or a separate one.

Depending on the [environment](../envs/README.md) you specified, you will need the number agents listed in it, unless they are external.

## How it works

The `agent` once running will connect to the control plane. If the control plane goes/offline or isn't online yet that's okay! The `agent` will continuously try to reconnect to the control plane endpoint provided.

For running `snarkOS` the `agent` will download that binary from the `control plane`.

## Updating

You don't have to worry about updating the individual agents.

The control plane will serve the updated `agent` binary to them once there is an update.

## Startup Options

The options for starting up an `agent`.

### _endpoint_

Is a optional argument that can be provided via the CLI or the `SNOPS_ENDPOINT` environment variable.

> NOTE: If both the CLI flag and the ENV variable are present, the ENV variable takes precedence over the CLI flag.

If not provided it will default to `127.0.0.1:1234`.

This is the endpoint of the `Control Plane`.

If you want it to be a secure connection please specify `https://` or `wss://` at the beginning of the endpoint or it will default to `http` and `ws`.

### _id_

The field where you can give this agent a specific ID, so it is identifiable within the `snops` ecosystem.

### _labels_

Optional comma separated list of labels you can apply to the agent, which are used for filtering and grouping.

### private-key-file

An optional private key file. If provided is used when starting `snarkOS`.

### path

Optional path to the directory containing the stored data and configuration for the agent.

By default it is `snops-data` local to where the agent was run from.

### external

TODO

### internal

TODO

### bind_addr

The optional address to bind to when running.

Defaults to `0.0.0.0`.

### node

Optional port for the `snarkOS` node server to run on.

Defaults to `4130`.

### bft

Optional port for the `snarkOS` BFT to run on.

Defaults to `5000`.

### rest

Optional port for the `snarkOS` REST API to run on.

Defaults to `3030`.

### metrics

Optional port for the `snarkOS` metrics to run on.

Defaults to `9000`.

### validator

Enables `validator` mode as an option for the agent's `snarkOS` node.

### prover

Enables `prover` mode as an option for the agent's `snarkOS` node.

### client

Enables `client` mode as an option for the agent's `snarkOS` node.

### compute

Enables `compute` mode as an option for the agent to be able to run transactions fired from within `snops`.

### quiet

Run the agent in quiet mode which prevents `snarkOS` node output.