## Grafana Setup for Prometheus and Loki

### 1. Login

1. Login to Grafana at [http://localhost:3000](http://localhost:3000)
2. Enter default credentials: `admin` for username and password
3. The Datasources and dashboard have already been configured for your convenience

## 2. Snops

Add the `--prometheus http://localhost:9090` and `--loki http://localhost:3100` flags to your control plane.
