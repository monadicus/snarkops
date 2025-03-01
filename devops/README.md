# Kubernetes Integration

## Developer Environment

### Prerequisites

1. [Docker](https://www.docker.com/)
1. [`kind`](https://kind.sigs.k8s.io/) (Kubernetes in docker)
1. [`helm`](https://helm.sh/) (Kubernetes package manager)

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
    kind load docker-image snops:latest snops-agent:latest
    ```

4. Install snops and agents into the Kubernetes cluster

    ```bash
    helm dependencies build ./devops/helm/snops
    helm --kube-context kind-kind install snops ./devops/helm/snops
    ```

5. Deploy snarkops environment
    ```bash
    kubectl --context kind-kind exec -it deployments/snops-controlplane -- scli env apply - <specs/testnet-4-validators.yaml
    ```

6. Execute a transaction

    ```bash
    kubectl --context kind-kind exec -it deployments/snops-controlplane -- scli env action execute transfer_public example.aleo 123u64
    ```

7. Verify a balance

    ```bash
    kubectl --context kind-kind exec -it deployments/snops-controlplane -- scli env balance example.aleo
    ```

### Teardown

1. Delete snarkops environment

    ```bash
    kubectl --context kind-kind exec -it deployments/snops-controlplane -- scli env apply delete
    ```

1. Uninstall snops from the Kubernetes cluster

    ```bash
    helm --kube-context kind-kind uninstall snops
    ```

1. Delete `kind` cluster

    ```bash
    kind delete cluster
    ```
