# Kubernetes Integration

## Developer Environment

### Prerequisites

1. [Docker](https://www.docker.com/)
1. [`kind`](https://kind.sigs.k8s.io/) (Kubernetes in docker)

### Start Environment

1. Build snops containers

    ```bash
    cargo xtask containers
    ```

2. Create development Kubernetes cluster

    ```bash
    kind create cluster
    ```

3. Load locally built containers into `kind`

    ```bash
    kind load docker-image snops snops-agent
    ```

4. Install snops and agents into the Kubernetes cluster

    ```bash
    kubectl --context kind-kind apply -k devops/k8s
    ```

5. Deploy snarkops environment
    ```bash
    kubectl --context kind-kind exec -it deployments/snops-controlplane -- scli env apply - <specs/testnet-4-validators.yaml
    ```

6. Execute a transaction

    ```bash
    kubectl --context kind-kind exec -it deployments/snops-controlplane -- scli env action execute transfer_public example.aleo 1u64
    ```

### Teardown

1. Delete snarkops environment

    ```bash
    kubectl --context kind-kind delete -k devops/k8s
    ```

1. Uninstall snops from the Kubernetes cluster

    ```bash
    cat devops/snops.*.yaml | kubectl --context kind-kind delete -f -
    ```

1. Delete `kind` cluster

    ```bash
    kind delete cluster
    ```
