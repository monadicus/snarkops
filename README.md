# snarkos-test

Crates for AOT transaction generation and repeatable testing infrastructure.

Requires a local clone of `snarkos` and `snarkvm` in the parent directory. That
is, your file tree should look like:

```
- <parent folder>
  - snarkos
  - snarkos-test
  - snarkvm
```

## Scripts

In order to use the [test scripts](/scripts/), you must build `snarkos` in
release mode with `cargo build --release` from the `snarkos` directory.
