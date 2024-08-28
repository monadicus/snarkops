[environment]: ../envs/README.md

# Control Plane

The `control plane` is responsible for:

- Communicating to `agents`.
- Delegating work to appropriate `agents`.
- Various [actions](../../glossary/ACTIONS.md) you can perform on agents and within an [environment].
- Searching among agents.
- Forwarding logs and metrics.
- Serving `snarkos-aot` and `agent` binaries.
- Storing and querying [environment] information.

The binary has several options you can run mess with at launch and some are required that we will go over here.

However, for a more in depth information you can read about the CLI options [here](../clis/SNOPS.md).

## Running the Control Plane

The `control plane` is the critical part of `snops` as it delegates and handles all incoming information from `agents`.

### Startup Options

The binary has several options you can run mess with at launch and some are required that we will go over here.

However, for a more in depth information you can read about the CLI options [here](../clis/SNOPS.md).

#### bind_addr

The optional address for the `control plane` to bind to when running.

Defaults to `0.0.0.0`.

#### port

The optional address for the `control plane` to bind to when running.

Defaults to `1234`.

#### prometheus

Is a optional argument that can be provided via the CLI or the `PROMETHEUS_URL` environment variable.

> NOTE: If both the CLI flag and the ENV variable are present, the ENV variable takes precedence over the CLI flag.

If provided it will send metric data to [prometheus](https://prometheus.io/).

#### loki

Is a optional argument that can be provided via the CLI or the `LOKI_URL` environment variable.

> NOTE: If both the CLI flag and the ENV variable are present, the ENV variable takes precedence over the CLI flag.

If provided it will send log data to [Loki](https://grafana.com/oss/loki/).

#### prometheus_location

Optional value of where `prometheus` is located.

This option only matters if you are running the `agents` local to the `control plane`.

The default is `docker`.

The other two options are `external` and `internal`.

> WARNING: For locally run `agents` an external prometheus server is not yet supported.

#### path

Optional path to the directory containing the stored data and configuration for the agent.

By default it is `snops-control-data` local to where the `control plane` was run from.

#### hostname

The optional hostname(IP or FQDN) for an external `cannon`/`compute`.

It must include `http://` or `https://`.

## Updating

To update the `control plane` simply stop the current one, and replace the binary.

When you run the `control plane` again it will read it's data back from the `path` specified during running.
