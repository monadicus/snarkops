# Architecture

A snops "instance" is composed of multiple parts:

- A Control Plane
- Agents
- A runner which is:
snarkos-aot: our own modified runner and aot wrapper around the official [snarkOS](https://github.com/AleoNet/snarkOS).

<!-- TODO nice to have eventually -->
<!-- In order to instruct the control plane after it has been started, you can use
