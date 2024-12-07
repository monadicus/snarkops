## Local Development

### Prereqs
1. Install [`kind`](https://kind.sigs.k8s.io/)

### Setup

1. `cargo xtask containers` - Build snops containers
2. `kind create cluster` - Create development Kubernetes cluster
3. `kind load docker-image snops snops-agent`
4. `cat devops/snops.*.yaml | kubectl --context kind-kind apply -f -`
5. Deploy environment
    ```bash
    kubectl --context kind-kind exec -it deployments/snops-controlplane -- scli env apply - <specs/testnet-4-validators.yaml
    ```

### Teardown

1. Destroy environment

    ```bash
    kubectl --context kind-kind exec -it deployments/snops-controlplane -- scli env delete
    ```

1. Destroy pods

    ```bash
    cat devops/snops.*.yaml | kubectl --context kind-kind delete -f -
    ```

1. `kind delete cluster` - Delete cluster
