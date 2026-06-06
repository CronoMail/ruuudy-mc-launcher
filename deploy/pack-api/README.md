# Standalone Pack API Deploy

The root `docker-compose.yml` runs the launcher pack API separately from StreamRelay.

It still joins Docker network `streamrelay_default` with alias `pack-api`, so the existing Caddy route can keep:

```caddyfile
launcher.ruuudy.in, http://launcher.ruuudy.in {
    reverse_proxy pack-api:8787
}
```

## VM Layout

Expected VM directory:

```text
/opt/ruuudy-mc-launcher
  admin-token.txt
  docker-compose.yml
  package.json
  server/
    manifest-server.mjs
    data/
```

## Deploy

From the VM:

```bash
cd /opt/ruuudy-mc-launcher
docker compose up -d
docker logs --tail 40 ruuudy-pack-api
curl -fsS https://launcher.ruuudy.in/health
```

The container runs as uid/gid `1001:1001` by default, matching the Oracle VM `ubuntu` user.

## Updating

Make API fixes locally in the launcher repo, push to GitHub, then update `/opt/ruuudy-mc-launcher` on the VM and restart only this service:

```bash
cd /opt/ruuudy-mc-launcher
docker compose restart pack-api
```
