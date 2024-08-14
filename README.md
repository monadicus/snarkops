<!-- TODO snops image <p align="center">
  <a href="https://monadic.us/">
    <img width="90%" alt="snops" src="">
  </a>
</p> -->

<h1 align="center">
  snarkOPs
</h1>

This repository is home to the `snops` (snarkOS operations) ecosystem, and
`snarkos-aot`, a crate for performing ahead-of-time ledger actions as well as
a plethora of developer tooling.

snops is a suite of tools that can be used to maintain [Aleo](https://aleo.org/)
network environments. The environments are defined in a manner similar to
infrastructure-as-code, which means you can create repeatable infrastructure
using the environment schema.

This can be used to create devnets, simulate events on the network like outages
and attacks, and guarantee metrics.

To learn more about `snops` we recommend checking out the mdbook [here](https://monadicus.github.io/snarkops/).

## Quickstart

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

### Trivial Local Isonets

This example requires 4 agents and the control plane to be running.

1. Start the environment: `snops-cli env prepare specs/test-4-validators.yaml`
1. Check the current network height: `snops-cli env height`
1. Look at the latest block: `snops-cli env block`
1. Look at the genesis block: `snops-cli env block 0`
1. Stop the environment: `snops-cli env clean`

### Trivial Isonet Transfers

Using the setup for a [Local Isonet](#trivial-local-isonets), executing Aleo programs has never
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


### Trivial Isonet Program Deployments

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
