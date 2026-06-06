# Pelican Launcher-Pack Installer Design

## Goal

Add a Pelican plugin that installs and updates Minecraft servers from the same launcher API pack code used by friends' desktop launchers. Administrators can choose either a preserving install or a fresh wipe.

## Compatibility Guarantee

This feature must not change existing desktop-launcher behavior.

- Keep launcher manifests at `schemaVersion: 1`.
- Keep the existing `files[].side` field and its current launcher behavior.
- Add only optional fields that old and current launcher builds safely ignore.
- Keep `defaultOptions` client-only and exclude it from server installs.
- Add regression tests proving a manifest with server metadata produces the same desktop install plan as the same manifest without it.

## Manifest Contract

Normal pack manifests remain the source of truth. Server-related additions are optional:

```json
{
  "schemaVersion": 1,
  "serverPack": {
    "enabled": true,
    "preservePaths": [
      "world/**",
      "server.properties",
      "ops.json",
      "whitelist.json",
      "banned-*.json"
    ]
  },
  "overrides": [
    {
      "path": "config/example.toml",
      "url": "/api/packs/EXAMPLE/files/config/example.toml",
      "sha256": "...",
      "size": 123,
      "side": "both"
    }
  ]
}
```

Allowed distribution values are `both`, `client`, `server`, and `excluded`.

- Existing `files[].side` values are validated and consumed by the server installer.
- `overrides[].side` defaults to `both` when omitted, preserving compatibility with current published packs.
- `defaultOptions` is always client-only.
- `serverPack.enabled` defaults to `false` when omitted, preventing accidental server installs from unreviewed packs.

The Pack API exposes `GET /api/packs/:code/server-manifest`. It returns a filtered, normalized installation manifest containing only `both` and `server` files, resolved public URLs, hashes, sizes, loader information, and preserve defaults.

## Components

### Launcher Pack API

The existing standalone Pack API validates optional server metadata and produces a normalized server manifest. It never modifies desktop manifests while serving them.

### Installer Agent

A separate `ruuudy-pack-installer` Docker service runs on the Oracle VM. It has authenticated access to the Pack API and a bind mount for Pelican server data.

The agent:

- validates the requested server UUID and prevents path traversal;
- obtains the normalized server manifest;
- stages every file outside the live server directory;
- verifies hashes and sizes;
- creates a rollback snapshot;
- applies either preserve or wipe mode atomically;
- writes `.ruuudy-pack-install.json` with installed code/version and managed files;
- emits structured progress and a final report.

The agent never edits another Pelican server directory and refuses installation unless the target server is stopped.

### Pelican Plugin

The plugin adds a server-area page named `Launcher Pack`.

It shows:

- linked pack code and installed version;
- available pack version;
- Minecraft and loader versions;
- install/update action;
- `Preserve Server Data` and `Fresh Wipe` modes;
- optional start-after-install toggle;
- progress, logs, last report, and rollback action.

The plugin stops the server, requests a normal Pelican backup when possible, dispatches the installer-agent job, and optionally starts the server after success.

## Install Modes

### Preserve Server Data

The installer replaces pack-managed files while preserving:

- worlds and dimension folders;
- `server.properties`;
- ops, whitelist, bans, permissions, and user cache;
- backups;
- any explicit pack or administrator preserve patterns.

Files previously managed by the linked pack but removed in the new version are deleted unless they match a preserve rule.

### Fresh Wipe

The installer creates a rollback snapshot, clears the target server directory, then installs only the new server pack. Pelican-managed backup storage is outside the server directory and is never deleted.

## Loader Handling

The first version validates that Pelican already has a compatible loader installation:

- Minecraft version must match.
- Loader type and loader version must match.
- Required loader startup files such as `unix_args.txt`, Fabric launcher jar, or NeoForge args must exist.

If validation fails, the plugin stops before changing files and tells the administrator which Pelican egg variables/reinstall action are required. Automatic Pelican egg switching and reinstall orchestration can be added later without weakening the installer transaction.

## Failure Safety

- No live files change until every download passes size and hash validation.
- Install jobs use a per-server lock.
- Failed installs retain the rollback snapshot and detailed report.
- Successful installs remove temporary staging data.
- Rollback is explicit and never automatic after the server has restarted.
- API and agent tokens are stored as secrets and never returned to the browser.

## Testing

- Pack API contract tests cover validation, filtering, defaults, and URL normalization.
- Launcher regression tests prove optional server metadata does not affect desktop installation.
- Installer tests use temporary directories for preserve, wipe, update, removed-file cleanup, hash failure, lock contention, and rollback.
- Pelican plugin tests cover permissions, form validation, job dispatch, progress rendering, stop-before-install, and optional restart.
- Deployment smoke tests install a small fixture pack into a disposable Pelican server before enabling the plugin on a real server.

