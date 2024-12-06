# Running

This section is dedicated to running the different components of `snops`.

To run an [environment](../envs/README.md) with snops it requires having two elements deployed, and 3 binaries compiled/built.

The two binaries that need to be deployed are:
- [The Control Plane](./CONTROL_PLANE.md)
- [Agent(s)](./AGENT.md)
	- You will likely want more than one instance of an agent.

The last binary that needs to be compiled is the `snarkos-aot` binary, that wraps around [snarkOS](https://github.com/AleoNet/snarkOS) importing it as a library.

Additionally you can enable [metrics and logging](./METRICS_AND_LOGGING.md), to better track all information in `snops`:
- Logs from control plane.
- Logs from the agents.
- Metrics and logs from `snarkOS`.

<!-- TODO move to appropiate place and update

	<!-- Similarly, agents can run as a daemon on separate machines.

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

	### Starting agents

	<!-- TODO: agent containerization -->
<!--
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
	snops-cli env apply my-env-spec.yaml
	```

	In a dev environment, you can use the following script:

	```bash
	./scripts/env_start.sh specs/test-4-validators.yaml
	```

	### Starting the control plane

	To start the control plane, build and execute the `snops` crate binary or use

	```bash
	./scripts/control_plane.sh
	``` -->
