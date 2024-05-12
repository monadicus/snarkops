# Timelines

The timeline document is where you can specify a multitude of different actions for the nodes to take.

This is an optional document at `environment` prepare time.
You an also add timelines after an `environment` has started.

## Fields

The different top level fields you can specify in a timeline document and what they mean. You can skip to the different actions by clicking [here](#timeline-actions). You can skip to examples by clicking [here](#examples).

The required fields are italicized.

### _version_

The version of the timeline document.

### _name_

The name of the timeline document.

Should we remove this??? Our commands already put the timeline id and name is never used?

### description

The optional description for a topology document.

### timeline

Where you list the different actions of the timeline.

## Timeline Actions

The different kinds of actions you can have nodes perform.

Note there are two kinds of actions:

- `awaited`: if awaited it  forces the step to wait until the condition is met rather than applying the change immediately.
- and regular does not wait until the condition is met.

For, every action you can also specify a:

- `duration`: is the minimum time until the next action is started.
- `timeout`: how long the action should take. If it goes over it kills the timeline.

### online

An action where you can specify `Node Target(s)` to go online.

If targets are not provided turns all nodes online.

### offline

An action where you can specify `Node Target(s)` to go offline.

If targets are not provided turns all nodes offline.

### config

An action where you can modify `Node Target(s)` configuration.

This includes changing their `height`, `peers` or `validators`(validator nodes only).

### cannon

You can spawn a cannon to fire transactions at a node.

> [NOTE] Requires that the environment was prepared with a cannon document.

A `name` is required.

You can also optionally specify:

- the `count` of tx's
- `query` to overwrite the query's source node
- `sink` to overwrite the cannon sink target

## Examples

A few different examples of timeline docs.

### Toggle Nodes Timeline

```yaml
---
version: timeline.snarkos.testing.monadic.us/v1

name: toggle-nodes

timeline:
  - duration: 5s # after 5 seconds of running
  - offline: "*/*" # turns all nodes offline
    duration: 5s # for 5 seconds
  - online: "*/*" # turns all nodes online
```

### Turn off Single Node

```yaml
---
version: timeline.snarkos.testing.monadic.us/v1

name: turn-off-single-node-or-else

timeline:
  # .await forces the step to wait until the condition is met rather than applying the change immediately
  - offline.await: */*
    # wait at most 10 seconds for the nodes to go offline or kill the timeline
    timeout: 10s
```

### Change a Node Config

```yaml
---
version: timeline.snarkos.testing.monadic.us/v1

name: change-node-config

timeline:
  client/1:
    # rollback the ledger by an hour (based on block timestamp deltas)
    height: 1hr
```

### Change Multiple Node Configs

```yaml
---
version: timeline.snarkos.testing.monadic.us/v1

name: change-multiple-node-configs

timeline:
  validator/1,validator/2:
    # disconnect all peers
    peers: []
    # set validators
    validators: [validator/3, validator/4]
    # reset ledger completely
    height: 0

  # affect all clients
  client/*:
    # set validators 1 and 2 as peers
    peers: [validator/1, validator/2]
    # set all clients to block 100
    height: 100
```

### Simple Cannon Example

This one also uses a `cannon` document.

```yaml
---
version: cannon.snarkos.testing.monadic.us/v1

name: committee-tx-public

source:
  file-name: txs.json

sink:
  target: validator/test-1
  tx-delay-ms: 1000

---
version: timeline.snarkos.testing.monadic.us/v1

name: tx-local

timeline:
  - cannon.await:
      - name: committee-tx-public
        count: 10
  - offline.await:
      - validator/test-0
      - validator/test-2
      - validator/test-3
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
