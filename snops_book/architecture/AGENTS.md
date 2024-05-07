# Agents

Machines that run the `snops-agent` crate.

Communicates with the control plane to receive state reconciliations and other messages.

Agents are in charge of:

- Running a [node](./RUNNERS.md).
  - Validators
  - Clients
  - Provers
- Being a compute. For example generating transactions.
- Send off transactions.