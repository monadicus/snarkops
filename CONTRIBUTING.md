# Contributing

This file describes the process for contributing to `snarkops`.

## Starting

We use the [xtask convention](https://github.com/matklad/cargo-xtask) to add helpful commands for development.
We also recommend usage of [cargo-watch](https://github.com/watchexec/cargo-watch), to help for development purposes.

The `xtask` commands can be triggered by doing `cargo xtask cmd`.
The list of commands are as follows, and you can always do `--help` for more info:
- `clipages`: updates the markdown files in `./snops_book/user_guide/clis`.
- `mangen`: generates man files and puts them in the target directory.
- `fmt`: can be used to format the codebase, requires nightly, or check the formatting is ok.
- `lint`: runs clippy against the codebase, requires nightly.
- `udeps`: installs [cargo-shear](https://github.com/boshen/cargo-shear) to do a naive unused dependencies check.
- `install-upx`: linux only command to install [upx](https://github.com/upx/upx), which can be used in the build command to super compress binaries.
- `build: gives makes building the different binaries easier, by automatically setting flags and etc for specific linkers, cranelift, or other options.
- `dev`: runs [cargo-watch](https://github.com/watchexec/cargo-watch) for the specified binary target.
<!-- TODO DEVELOPING DOCS -->
<!-- Then please read the
[developing instructions](https://github.com/monadicus/snarkops/blob/main/DEVELOPING.md) for setting up your environment. -->

<!-- ## Commits

Your commits must follow specific guidelines. -->

<!-- TODO IDR if we enforce this -->
<!-- ### Signed Commits

Sign all commits with a GPG key. GitHub has extensive documentation on how to:

- [Create](https://docs.github.com/en/authentication/managing-commit-signature-verification/generating-a-new-gpg-key)
  a new GPG key.
- [Add](https://docs.github.com/en/authentication/managing-commit-signature-verification/A-a-gpg-key-to-your-github-account)
  a GPG key to your GitHub.
- [Sign](https://docs.github.com/en/authentication/managing-commit-signature-verification/A-a-gpg-key-to-your-github-account)
  your commits. -->

### Convention

All commits are to follow the
[Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/) standard.
Commit messages should always be meaningful.

## Getting Ready For a PR

This section describes actions to keep in mind while developing.

### Formatting and Cleanliness

Please ensure your code is formatted and the formatting tool gives no warnings(if using a local snarkos/vm you can ignore warnings given from those repos).

## PRs

For creating the PR, please follow the instructions below.

1. Firstly, please open a
   [PR](https://github.com/monadicus/snarkops/pulls) from your branch
   to the `main` branch of `snarkops`.
2. Please fill in the PR template that is there.
3. Then assign it to yourself and anyone else who worked on the issue with you.
4. Make sure all CI tests pass.
5. Finally, please assign at least two of the following reviewers to your PR:
   - [gluax](https://github.com/gluax)
   - [Meshiest](https://github.com/Meshiest)
   - [voximity](https://github.com/voximity)
