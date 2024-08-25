### Environments

This section is about the different documents. Documents are a section of specific structured data in a `yaml` file that describe to the control plane how to orchestrate agents for said environment. The document sections are each identified by a `version: storage.snarkos.testing.monadic.us/v1`. Where the first part is the document name, and the last part is the version of the document.

Some documents are required for an environment to work. While others are optional and can be applied at the start or after an environment has been started.

This means you have multiple documents in a single file at times.

To learn more about what each environment controls read about it [here](../../architecture/CONTROL_PLANE.md#environments).

