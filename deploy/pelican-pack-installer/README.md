# Pelican Pack Installer Agent

This internal service installs launcher API packs into stopped Pelican Minecraft server directories.

It is intentionally separate from the launcher Pack API and Pelican panel. The service must only be
reachable from Pelican's internal Docker network.

## Required Environment

```text
PACK_INSTALLER_TOKEN=<long random secret>
PACK_API_BASE=https://launcher.ruuudy.in
PELICAN_DOCKER_NETWORK=<actual Pelican Docker network>
```

## Deploy

```bash
cd /opt/ruuudy-mc-launcher/deploy/pelican-pack-installer
docker compose -f compose.yml up -d --build
docker exec ruuudy-pack-installer wget -qO- http://127.0.0.1:8790/health
```

The agent refuses unsafe server IDs and paths, stages and verifies every file, then retains rollback
data below `/minecraft/servers/.ruuudy-pack-installer/<server UUID>/rollbacks`.

