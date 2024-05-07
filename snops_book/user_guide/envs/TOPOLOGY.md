# Topology

The topology document is where you can specify:

- Internal connections(`agents` controlled by the `control plane`)
- External connections(other nodes outside the `control plane`)

The topology document is required for a `environment` to run.

## Fields

The different top level fields you can specify in a topology document and what they mean. You can skip to examples by clicking [here](#examples).

The required fields are italicized.

### _version_

The version of the topology document.

### _name_

The name of the topology document.

Should we remove this??? Our commands already put the env id and name is never used.

### description

The optional description for a topology document.

### external

Where you can optionally list exeternal nodes, i.e. nodes outside of the `control plane's` control.

If they are running with the default ports you can simply provied a node key(validator/foo@some_company_name) with their ip.

For, reference the default ports are as follows:

- `bft: 5000`
- `node: 4130`
- `rest: 3030`

```yaml
external:
  validator/alpha@canary: 3.111.151.121
  client/alpha-0@canary: 52.86.189.144
```

If you do need to specify the socket address(ip and port) for the `bft`, `node` and `rest` connections.

```yaml
external:
  validator/alpha@canary:
    bft: 3.111.151.121:4040
    node: 3.111.151.121:3777
    rest: 3.111.151.121:5555
  client/alpha-0@canary: # this will use the default port for node, but the ip from bft
    bft: 52.86.189.144:4040
    rest: 52.86.189.144:5555
```

### internal

Where you can optionally list internal nodes, i.e. `agents` controled by the plane that will run the provided mode. You will need the corresponding number of agents to create those nodes and their replicas.

#### online

An optional value to specify if a node is online or not.
`true` is online.
`false` is offline.
Defaults to `true`.

#### replicas

An opitional field that when specified, creates a group of nodes, all with the same configuration.
So it will require that many agents to be running.

#### key

The private key for the node to use.

#### height

An optional field that when:
- not provided crates a new ledger when the block is started.
- `top` will use the latest heigth for the ledger.
- a number to say what height to start at. If set to `0` resets the height to the genesis block.
- or the next checkpoint that matches the retention span.


#### labels

An optional list of labels to provide to the node.

#### agent

An optional `AgentId` that if specified this node has to use that agent.

#### validators

An optional, list of validators to connect to:
- single `NodeTarget`, i.e. `validator/2`
- list of targets, i.e. `[validator/2, validator/3]` or `[clients.2-5]`(all clients).

> [NOTE] only applicable if the node is run in validator mode.

#### peers

An optional, list of peers to connect to:
- single `NodeTarget`, i.e. `client/2`
- list of targets, i.e. `[client/2, client/3]` or `[clients.$]`(all clients).

#### env

An optional list of environment variables to provide to the node.

`RUST_BACKTRACE: 1`

## Examples

A few different examples of topology docs.

### Four Validators

```yaml
version: nodes.snarkos.testing.monadic.us/v1
name: 4-validators

nodes:
  validator/test:
    # this requires 4 agents
    replicas: 4
    key: committee.$
    height: 0
    validators: [validator/*]
    peers: []
```

### External Connections and a Client

```yaml
version: nodes.snarkos.testing.monadic.us/v1
name: 4-clients-canary

external:
  validator/1@canarynet:
    node: 11.12.13.14:4130

  validator/2@canarynet:
    node: 11.12.13.14:4130

  client/1@canarynet:
    node: 11.12.13.14:4130

nodes:
  client/test:
    key: extra.$
    replicas: 4
    height: 0
    validators: []
    # has all of every type of node that are at canarynet as peers
    peers: ["*/*@canarynet"] # so both validators and the client. 
```