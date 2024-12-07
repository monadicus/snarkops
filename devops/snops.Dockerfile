FROM rust:1.82-bookworm AS builder

ENV DEBIAN_FRONTEND=noninteractive
RUN set -eux; apt-get update -y
RUN apt-get install -y --no-install-recommends clang

WORKDIR /usr/src/snops
COPY .cargo .cargo
COPY .git .git
COPY crates crates
COPY Cargo.lock Cargo.lock
COPY Cargo.toml Cargo.toml

# RUN cargo xtask build aot
RUN cargo xtask build cli
RUN cargo xtask build control-plane
RUN cargo xtask build agent

FROM debian:bookworm-slim
RUN apt-get update && apt-get upgrade
RUN apt-get install --no-install-recommends -y ca-certificates openssl
RUN rm -rf /var/lib/apt/lists/*
RUN mkdir -p /etc/snops

COPY --from=builder /usr/src/snops/target/release-big/snops /usr/local/bin/snops
COPY --from=builder /usr/src/snops/target/release-big/snops-cli /usr/local/bin/scli
# COPY --from=builder /usr/src/snops/target/release-big/snarkos-aot /etc/snops/snarkos-aot
COPY --from=builder /usr/src/snops/target/release-big/snops-agent /etc/snops/snops-agent

COPY ./target/release-big/snarkos-aot /etc/snops/snarkos-aot
# COPY ./target/release-big/snops-agent /etc/snops/snops-agent
# COPY ./target/release-big/snops-cli /usr/local/bin/scli
# COPY ./target/release-big/snops /usr/local/bin/snops


CMD ["snops"]