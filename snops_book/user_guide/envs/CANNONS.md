# Cannons

The cannon document is an optional where you can specify:

- where to source transactions from (ahead-of-time generated file or generate in realtime)
- what kinds of transactions to generate
- where to send transactions (to a file, or a node in the topology)


The cannon document is not required for a `environment` to run, but the document needs to be present at `prepare` time to work.

This document is required if you want to use a [cannon timeline action](TIMELINES.md#cannon).

You can have more than one cannon per `environment`.

> [NOTE] For now only `credits.aleo` is supported.

## Fields

The different top level fields you can specify in a cannon document and what they mean. You can skip to examples by clicking [here](#examples).

The required fields are italicized.

### _version_

The version of the cannon document.

### _name_

The name of the cannon document.

This is so you can refer to this cannon in other documents.

### description

The optional description for a cannon document.

### _source_

Where the transactions should come from.

There are several modes to chose from for a source.

The type of source is determined by the options you provide.

> [NOTE] You can only select one mode.

#### playback

This `source` mode reads transactions from a file.

##### _file-name_

The name of the file to read transactions from.

```yaml
source:
  file-name: txs.json
```

The format of the file is:

```json
{tx_1_info...}
{tx_2_info...}
```

#### realtime

This `source` mode generates transactions in real time.

##### query

Sets the query service for the cannon to use.

Has two modes local ledger or a node in the `environment`.
Optional defaults to the local ledger.

###### local

An optional field that if provided uses the node in the `environment` specified to sync from.

Defaults to `None`, i.e. agent uses it's own local ledger as is.

```yaml
source:
  query:
    mode:
      sync-from: client/1 # optional
```

###### node

An optional field that if provided uses the node in the `environment` specified pulls that node's state root over RPC.

```yaml
source:
  query:
    mode: client/1 # required
```

##### compute

Sets the compute service for the cannon to use.

Has two modes agent or demox.
Optional defaults to the agent.

###### agent

This tells the cannon to use agents in the `environment`.

You can optionally provide a list of agent labels to specify which agents to use.

```yaml
source:
  compute:
    labels: foo,bar
```

###### demox

This tells the cannon to use Demox's API to generate the executions.

Requires the url for the API.

```yaml
source:
  compute:
    demox-api: https://exampl_url.com/api/v1
```

##### tx-modes

The transaction methods to call.

Optional defaults to `credits.aleo/transfer_public`.

```yaml
source:
  tx-modes: [transfer-public]
```

> [NOTE] Only `credits.aleo` is supported at this time. And only transfer transactions.

##### private-keys

The private keys of the accounts that will make the transaction method call.

Optional defaults to committee keys.

```yaml
source:
  private-keys: [committee.$]
```

##### addresses

The addresses of the accounts that will recieve the transfer.

Optional defaults to committee keys.

```yaml
source:
  addresses: [committee.$]
```

#### listen

This `source` mode receive authorizations from a persistent path, `/api/v1/env/:env_id/cannons/:id/auth`.

The same query mode as [above](#query).
The compute mode as [above](#compute).

However, they are now both requried.

```yaml
source:
  query:
    mode:
      sync-from: client/1 # optional
  compute:
    labels: foo,bar # optional
```

### _sink_

Where the transactions should go to.

There are several modes to chose from for a sink.

The type of sink is determined by the options you provide.

> [NOTE] You can only select one mode.

#### record

This `sink` mode writes txs to a file.

##### _file-name_

The name of the file to write transactions to.

```yaml
sink:
  file_name: txs.json
```

The format of the file is:

```json
{tx_1_info...}
{tx_2_info...}
```

##### tx_request_delay_ms

An opitional field to specify how long between writes the tx should take, in milliseconds.

Defaults to `1000`

```yaml
sink:
  file-name: ...
  tx-request-delay-ms: 1000
```

#### realtime

This `sink` mode sends the txs to a node in the env.

##### _target_

The node target(s) the tx's shoud be fired at.

```yaml
sink:
  target: client/1
  rate: ...
```

##### _rate_

The fire rate that tx's shoud be fired at.

Read more about [fire rates](../../glossary/FIRE_RATE.md)

```yaml
sink:
  target: ...
  rate:
    tx-delay-ms: 5000
```

### instance

An optional value to specify if the cannon shoud be made when the document is loaded.
`true` means the cannon is created immediately upon preparing an `enviroment`.
`false` means it will be prepared when a `timeline` tells it to.
Defaults to `false`.

### count

Number of transactions to fire when for an instanced cannon is created.

## Examples

A few different examples of topology docs.

### Realtime Record Right Away

```yaml
---
version: cannon.snarkos.testing.monadic.us/v1

name: realtime-txs-record-to-file

# realtime mode
source:
  tx-modes: [transfer-public]
  private-keys: [committee.$]
  addresses: [committee.$]

sink:
    file-name: txs.json

# create the cannon immediately
instance: true
# fire 10 transactions
count: 10
```

### Playback Fire Right Away
```yaml
---
version: cannon.snarkos.testing.monadic.us/v1

name: txs-from-file-to-target-node

# playback mode
source:
  file-name: txs.json

# realtime mode
sink:
  target: validator/test-1
  tx-delay-ms: 1000

# create the cannon immediately
instance: true
# fire 10 transactions
count: 10
```

### TODO more examples