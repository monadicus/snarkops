FROM debian:bookworm-slim
RUN apt-get update && apt-get upgrade
RUN apt-get install --no-install-recommends -y ca-certificates curl
RUN rm -rf /var/lib/apt/lists/*

RUN mkdir -p /var/opt/snops-agent
RUN mkdir -p /etc/snops-agent

COPY devops/agent-entrypoint.sh /usr/local/bin/agent-entrypoint.sh
RUN chmod +x /usr/local/bin/agent-entrypoint.sh

CMD ["/usr/local/bin/agent-entrypoint.sh"]
