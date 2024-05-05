# Running

TODO move to appropiate place and update

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

	### Starting the control plane

	To start the control plane, build and execute the `snops` crate binary or use

	```bash
	./scripts/control_plane.sh
	``` -->
