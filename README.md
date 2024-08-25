<!-- TODO snops image <p align="center">
  <a href="https://monadic.us/">
    <img width="90%" alt="snops" src="">
  </a>
</p> -->

<h1 align="center">
  snarkOPs
</h1>

[Snops Quickstart](#snops-quickstart) | [snarkos-aot Quickstart](#snarkos-aot-quickstart)

This repository is home to the `snops` (snarkOS operations) ecosystem, and
`snarkos-aot`, a crate for performing ahead-of-time ledger actions, transaction
authorizations, and various tools for helping with developing Aleo programs.

snops is a suite of tools that can be used to maintain [Aleo](https://aleo.org/)
network environments. The environments are defined in a manner similar to
infrastructure-as-code, which means you can create repeatable infrastructure
using the environment schema.

This can be used to create devnets, simulate events on the network like outages
and attacks, and guarantee metrics.

To learn more about `snops` we recommend checking out the mdbook [here](https://monadicus.github.io/snarkops/).

## Snops Quickstart

### Easy Setup

`snops` contains several components that can be compiled separately. The `release-big` build profile allows for faster near-release profile performance with faster near-debug profile build times.

1. Install [rust](https://www.rust-lang.org/)
1. Clone the repo
1. Start + build the control plane: `./scripts/control_plane.sh`

    The controlplane is the webserver that communicates to agents how to
    run snarkOS, or what transactions to execute.

1. In another terminal, build the cli: `cargo install --path ./crates/snops-cli`

    The cli is used to interact with the controlplane and manage environments.
    It provides JSON based output. We recommend pairing our cli with [`jq`](https://jqlang.github.io/jq/) when leveraging other scripts and tools

1. Build the agent: `cargo build --profile release-big -p snops-agent`

    The agent is a lightweight service that starts up `snarkos-aot` which
    automatically configures snarkos nodes, or executes transactions.

1. Build `snarkos-aot` (for running nodes): `cargo build --profile release-big -p snarkos-aot`

    `snarkos-aot` is an alternative snarkOS CLI providing more developer-oriented
    features as well as tooling for distributed transaction generation and execution.

1. In separate terminals, start up some agents with the following commands:

    ```sh
    ./script/agent.sh 0
    ./script/agent.sh 1
    ./script/agent.sh 2
    ./script/agent.sh 3
    ```

    Each of these can be dynamically configured as snarkos nodes. The default
    agent configuration should connect to a locally operated controlplane.

### Local Isonets

This example requires 4 agents and the control plane to be running.

1. Start the environment: `snops-cli env prepare specs/test-4-validators.yaml`
1. Check the current network height: `snops-cli env height`
1. Look at the latest block: `snops-cli env block`
1. Look at the genesis block: `snops-cli env block 0`
1. Stop the environment: `snops-cli env clean`

### Isonet Transfers

Using the setup for a [Local Isonet](#local-isonets), executing Aleo programs has never
been more convenient. Snops aliases locally generated keys automatically from configuration and
reduces the need to keep track of individual key files.

1. Start an environment (see previous example)
1. Start a compute agent: `./scripts/agent_compute.sh 0`

    Compute agents are able to distribute the execution of transactions,
    offloading the compute task to different servers.

1. Check the balance of committee member 1's key:

    ```sh
    $ snops-cli env balance committee.1
    10000000000000
    ```

1. Transfer 1 credit to committee member 1 from committee member 0 (look on snarkos this command would require MANY more flags):

    ```sh
    snops-cli env action execute transfer_public committee.1 1_000_000u64
    ```

1. Check the balance of committee member 1's key (it may take a few seconds for the block to advance):

    ```sh
    snops-cli env balance committee.1
    10000001000000
    ```


### Isonet Program Deployments

Deploying and executing Aleo programs on your isonets is easiest with snops. You do not need any extra json files or tooling, only a standalone `.aleo` file.

1. Use the same setup as before
1. Create a file `mapping_example.aleo` with the following contents:

    ```js
    program mapping_example.aleo;

    mapping mymapping:
      key as u8.public;
      value as u8.public;

    function store:
        input r0 as u8.public;
        input r1 as u8.public;
        async store r0 r1 into r2;
        output r2 as mapping_example.aleo/store.future;

    finalize store:
        input r0 as u8.public;
        input r1 as u8.public;
        set r1 into mymapping[r0];
    ```
1. Deploy the program: `snops-cli env action deploy ./mapping_example.aleo`
1. Verify the program is on the chain: `snops-cli env program mapping_example.aleo`
1. Check an example mapping
    ```sh
    $ snops-cli env mapping mapping_example.aleo mymapping 0u8
    {
      "value": null
    }
    ```
1. Execute the `store` function on-chain: `snops-cli env action execute mapping_example.aleo/store 0u8 5u8`
1. Check the example mapping for its updated value
    ```sh
    $ snops-cli env mapping mapping_example.aleo mymapping 0u8
    {
      "value": "5u8"
    }
    ```


## SnarkOS-aot Quickstart

`snarkos-aot` provides various CLI tools to help with developing and executing
Aleo programs as well as interact with snarkOS ledgers.

Build `snarkos-aot` with: `cargo install --profile release-big -p snarkos-aot`.
The compiled binary can be found in `target/release-big/snarkos-aot`.

Use the `NETWORK` environment variable to specify `mainnet` (default),
`testnet`, or `canary`.

### Transaction Authorizations

Typically when executing snarkOS transactions you specify the program and inputs
and receive a proof. Behind the scenes, snarkVM is using a private key to
"authorize" the proof for the program execution, and "authorize" the proof for
the fee.

Creating authorizations is much quicker than executing the transaction, and the
process can be done airgapped or even on web clients.

```sh
# Create an authorization for a executing transfer_public to the "example.aleo" program for 1 microcredit
NETWORK=testnet snarkos-aot auth program --private-key <PK> credits.aleo/transfer_public example.aleo 1u64 > auth.json
# The same as above but with a different private key
NETWORK=testnet snarkos-aot auth program --private-key <PK> --fee-private-key <PK> credits.aleo/transfer_public example.aleo 1u64 > auth.json

# Create an authorization for deploying a program (from my_program.aleo)
NETWORK=testnet snarkos-aot auth deploy --private-key <PK> my_program.aleo
# Determine the cost to deploy a program (from stdin)
cat my_program.aleo | NETWORK=testnet snarkos-aot auth deploy --private-key <PK> - | NETWORK=testnet snarkos-aot auth cost -

# Execute an authorization from auth.json without broadcasting it (the - means stdin)
cat auth.json | NETWORK=testnet snarkos-aot auth execute - --query https://api.explorer.aleo.org/v1

# Get the cost of an authorization (program or deployment) without executing the transaction
cat auth.json | NETWORK=testnet snarkos-aot auth cost -

# Derive the transaction id from an authorization without executing it
cat auth.json | NETWORK=testnet snarkos-aot auth id -

```

### Program Helpers

`snarkos-aot` contains various tools to programmatically interact with aleo program.

If you don't have any Aleo programs you can download a program (`credits.aleo`)
from Aleo Explorer with the following commands. You can find a list of
programs on [Aleo Explorer's Programs List](https://explorer.aleo.org/programs).

```sh
NETWORK=testnet
PROGRAM=credits.aleo
# Download a program, then un-jsonify it
curl https://api.explorer.aleo.org/v1/$NETWORK/program/$PROGRAM | jq -r > $PROGRAM
```

Below are some `snarkos-aot` commands for interacting with programs.

```sh
# Get a program's id from a text file
$ snarkos-aot program id ./credits.aleo
credits.aleo

# Calculate the cost of deploying a program
$ snarkos-aot program cost ./example.aleo
2568400

# Get a list of imports for a program (output in a json format with --json)
$ snarkos-aot program imports ./staking_v1.aleo --json
["credits.aleo"]

# Get a list of functions (and respective inputs/outputs) for a program (jq for formatting)
$ snarkos-aot program functions ./domainnames.aleo --json | jq
{
  "validate_name": {
    "inputs": [
      "[u128; 4u32].private"
    ],
    "outputs": [
      "boolean.private"
    ]
  }
}
```

### Account Helpers

Need to generate a bunch of vanity accounts for testing?

```
# Generate 5 accounts that have addresses that start with `aleo1f00`
snarkos-aot accounts 5 --vanity f00
```

## Contributing

`snops` is free and open source. You can find the source code on
[GitHub](https://github.com/monadicus/snarkops) and issues and feature requests can be posted on
the [GitHub issue tracker](https://github.com/monadicus/snarkops/issues). If you'd like to contribute, please read
the [CONTRIBUTING](https://github.com/monadicus/snarkops/blob/main/CONTRIBUTING.md) guide and consider opening
a [pull request](https://github.com/monadicus/snarkops/pulls).

## License

The `snops` source and documentation are released under
the [MIT License
](https://github.com/monadicus/snarkops/blob/main/LICENSE).
