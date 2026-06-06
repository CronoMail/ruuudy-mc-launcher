# Ruuudy Pack API

This is the hosted manifest service for the launcher.

Friends enter a pack code in the desktop app. The app calls:

```text
GET /api/packs/FAKERSBOB
```

When you update a profile, the launcher/admin side publishes the locked manifest to:

```text
PUT /api/admin/packs/FAKERSBOB
Authorization: Bearer <PACK_ADMIN_TOKEN>
```

## Run On Oracle

```bash
cd /opt/ruuudy-mc-launcher
npm install --omit=dev
PACK_ADMIN_TOKEN='change-this-long-token' PORT=8787 npm run pack-server
```

For Docker on the Oracle VM, prefer the standalone root `docker-compose.yml` and run:

```bash
cd /opt/ruuudy-mc-launcher
docker compose up -d
docker compose restart pack-api
```

This keeps the launcher API separate from the StreamRelay compose stack while still exposing
the `pack-api` Docker network alias used by Caddy.

For PM2:

```bash
pm2 start server/manifest-server.mjs --name ruuudy-pack-api --update-env
pm2 save
```

Example Caddy reverse proxy:

```caddyfile
launcher.ruuudy.in {
  reverse_proxy 127.0.0.1:8787
}
```

## Data

Pack manifests live in:

```text
server/data/packs/<CODE>/manifest.json
```

Back this folder up. It is the source of truth for profile codes.

## Repair Existing Packs For Pelican

Older published packs may exist as launcher/client packs without the optional `serverPack`
metadata that the Pelican installer requires. Repair one pack in place with:

```bash
npm run repair-server-pack -- BENTOBOOB --dry-run
npm run repair-server-pack -- BENTOBOOB
```

The repair enables server installs, preserves normal world/operator files, promotes likely
shared CurseForge mod entries from `client` to `both`, and keeps obvious client assets such as
resource packs, shader packs, screenshots, and `options.txt` client-only.
