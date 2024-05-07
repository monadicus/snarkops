# Contributing

This file describes the process for contributing to `snarkops`.

## Starting

<!-- TODO DEVELOPING DOCS -->
<!-- Then please read the
[developing instructions](https://github.com/monadicus/snarkops/blob/main/DEVELOPING.md) for setting up your environment. -->

## Commits

Your commits must follow specific guidelines.

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
