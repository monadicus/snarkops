# Cannons

The cannon document is an optional where you can specify:

- how to create transactions.
- how to use transactions.
- where to send transactions.

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

There are several modes to chose from for a source:

TODO should do h4's for these and show the different fields.

- playback: reads transactions from a file.
- realtime: generates transactions in real time.
- listen: receive authorizations from a persistent path.

You can find examples of these down [below](#examples).

### _sink_

Where the transactions should go to.

TODO should do h4's for these and show the different fields.

There are several modes to chose from for a sink:

- record: writes txs to a file.
- realtime: sends the txs to a node in the env.

You can find examples of these down [below](#examples).

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
# no query or compute they are optional
source:
	# calls `aleo.credits/transfer_public`
	# optional defaults to transfer public
  tx-modes: [transfer-public]
	# private keys for making txs.
	# optional defaults to committee keys.
  private-keys: [committee.$]
	# addresses for transaction targets.
	# optional defaults to committee keys.
  addresses: [committee.$]

# record mode
sink:
	# the delay in milleseconds between writes of txs.
	# optional defaults to `1000`.
  tx-request-delay-ms: 1000
	# the requried name of the file to record it to
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
	# the required name of the file.
  file-name: txs.json

# realtime mode
sink:
	# the required tx target node.
  target: validator/test-1
	# the fire rate here is in repeat mode.
	# required the delay between sending a tx.
  tx-delay-ms: 1000

# create the cannon immediately
instance: true
# fire 10 transactions
count: 10
```

### TODO more examples