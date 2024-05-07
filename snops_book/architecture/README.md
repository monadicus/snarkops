# Architecture

A snops "instance" is composed of multiple parts:

- A Control Plane
- Agents
- A runner which can be either:

  - the official snarkos runner: [snarkOS](https://github.com/AleoNet/snarkOS)
  - snarkos-aot: our own modified runner and aot wrapper around `snarkOS`.

<!-- TODO nice to have eventually -->
<!-- In order to instruct the control plane after it has been started, you can use
