# Test-only namespace firewall; never spawned by Operon clients.
FROM alpine@sha256:5291449c3df73caf6ed85e649dec1b9e818b39a5d8c871e97afc13e9cd5e8fa8
RUN apk add --no-cache iptables iproute2 python3 tcpdump conntrack-tools
ENTRYPOINT ["/bin/sh", "-ec"]
