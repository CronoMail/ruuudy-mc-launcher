# Custom Minecraft Launcher Plan

## Goal

Build a custom launcher that replaces the broken CurseForge-code workflow. The launcher should let players install and update the exact mod set used by the Minecraft server, including Modrinth mods and external/direct-download mods that CurseForge share codes do not include.

The first target is a Windows desktop launcher for friends joining `mc.ruuudy.in`. The design should stay flexible enough to support Linux and macOS later.

## Why This Exists

CurseForge share codes are not a reliable source of truth for this server because they can miss externally added mods. The server pack needs its own manifest that lists every required client mod exactly.

The launcher should install from that manifest instead of relying on a third-party launcher export.

## Core Features

1. **Share Code Install**
   - Player opens launcher.
   - Player enters a short server pack code.
   - Launcher downloads the manifest for that code.
   - Launcher installs Minecraft version, Fabric loader, required mods, and config files.

2. **Modrinth-first Mod Library**
   - Manifest stores Modrinth project IDs and version IDs where possible.
   - Launcher downloads files from Modrinth directly.
   - Launcher validates files using hashes from Modrinth.

3. **External Mod Support**
   - Manifest can also include direct URLs for mods not available on Modrinth.
   - Each external file must include filename, SHA-256 hash, size, and source URL.
   - Launcher verifies hashes before marking install complete.

4. **One-click Update**
   - Launcher compares local installed manifest version with latest remote manifest.
   - It downloads only missing or changed files.
   - It removes old managed mods that are no longer in the manifest.

5. **Server Profile**
   - Launcher creates a dedicated game profile for the server.
   - It should not damage the user’s normal `.minecraft` folder.
   - Recommended install location:
     `%APPDATA%/.ruuudy-mc/profiles/<pack-slug>`

6. **Server Join Info**
   - Launcher displays server IP: `mc.ruuudy.in`
   - Optional later: launch directly into the server if auth/session handling is added.

7. **Discord Bot Integration**
   - The bot should stop posting CurseForge codes.
   - Instead it posts:
     - launcher download link
     - current server pack code
     - pending mod changes
   - The existing `/modpack pending` and `/modpack publish` flow can be reused.

## Manifest Design

Example manifest:

```json
{
  "schemaVersion": 1,
  "packId": "fakersbob",
  "packName": "Fakersbob",
  "version": "2026.05.22-1",
  "minecraftVersion": "1.21.1",
  "loader": {
    "type": "fabric",
    "version": "0.19.2"
  },
  "server": {
    "address": "mc.ruuudy.in",
    "port": 25565
  },
  "files": [
    {
      "type": "modrinth",
      "side": "both",
      "required": true,
      "projectId": "P7dR8mSH",
      "versionId": "example-version-id",
      "filename": "fabric-api.jar",
      "sha512": "..."
    },
    {
      "type": "external",
      "side": "client",
      "required": true,
      "name": "External Example",
      "filename": "external-example.jar",
      "url": "https://example.com/external-example.jar",
      "sha256": "...",
      "size": 123456
    }
  ],
  "overrides": [
    {
      "path": "config/example.toml",
      "sha256": "...",
      "url": "https://launcher.ruuudy.in/packs/fakersbob/files/config/example.toml"
    }
  ]
}
```

## Share Code Design

The share code should map to a hosted manifest.

Example:

```text
FAKERSBOB
```

API lookup:

```text
GET /api/packs/FAKERSBOB
```

Response:

```json
{
  "code": "FAKERSBOB",
  "latestVersion": "2026.05.22-1",
  "manifestUrl": "https://launcher.ruuudy.in/packs/fakersbob/manifest.json"
}
```

## Architecture

### Launcher App

Recommended stack:

- **Tauri** for smaller app size and better desktop integration.
- React or Svelte frontend.
- Rust backend handles file downloads, hashing, extraction, and process launch.

Alternative:

- **Electron** is easier if we want pure JavaScript, but it is heavier.

Recommendation: start with Tauri unless setup becomes annoying. A launcher should feel lightweight.

### Manifest API

Recommended first version:

- Small Node/Express API inside the Discord bot, or a separate service later.
- Stores pack manifests as JSON files.
- Exposes public read-only endpoints for launcher clients.
- Admin updates can happen manually first.

Future version:

- Pelican mod manager writes directly to this manifest service when mods are installed or removed.

### Storage

Simple first version:

- `data/minecraft-packs/fakersbob/manifest.json`
- `data/minecraft-packs/fakersbob/files/...`

Future:

- Move large files to object storage or GitHub Releases if needed.

## Install Flow

1. User enters share code.
2. Launcher fetches pack metadata.
3. Launcher downloads manifest.
4. Launcher checks local profile folder.
5. Launcher downloads Fabric installer/loader if needed.
6. Launcher downloads required mods.
7. Launcher validates hashes.
8. Launcher removes managed files no longer in manifest.
9. Launcher writes local installed manifest.
10. Launcher shows "Ready to Play".

## Update Flow

1. Launcher checks latest manifest version.
2. If current version differs, show update button.
3. Diff old manifest vs new manifest.
4. Download changed/new files.
5. Remove old managed files.
6. Keep user-owned files untouched.

## Safety Rules

- Never delete files outside the launcher-managed profile.
- Only delete files that were previously managed by the launcher.
- Verify every downloaded file hash.
- Refuse external downloads without hashes.
- Keep a local install log.
- If update fails midway, keep the previous manifest and show a repair button.

## UI Screens

1. **Welcome / Enter Code**
   - Code input
   - Install button

2. **Pack Overview**
   - Pack name
   - Minecraft version
   - Loader
   - Server address
   - Installed/current version

3. **Install / Update Progress**
   - Current file
   - Download progress
   - Hash verification status
   - Error details if something fails

4. **Ready Screen**
   - Play button
   - Open folder
   - Repair install
   - Copy server IP

5. **Settings**
   - RAM allocation
   - Java path
   - Install folder
   - Optional debug logs

## Discord Bot Changes

Replace CurseForge wording:

- `/modpack current` shows the launcher code and latest manifest version.
- `/modpack publish` no longer requires a CurseForge code.
- Optional argument: `note`.
- Bot posts:
  - launcher code
  - manifest version
  - changes since last publish
  - "Open the launcher and press Update"

Possible commands:

```text
/modpack channel #updates
/modpack log action:added mod:"Fabric API"
/modpack pending
/modpack publish note:"Added Cobblemon Additions"
/modpack current
/modpack history
```

## Pelican Integration

First version:

- Pelican mod manager logs changes to the Discord bot.
- The manifest is updated manually.

Better version:

- Pelican mod manager sends installed/removed mod metadata to the manifest API.
- Manifest API updates the server pack.
- Bot records pending change.
- You run `/modpack publish` when ready.

## MVP Scope

Build only what is needed to prove the system:

1. Create launcher app.
2. Support one pack code: `FAKERSBOB`.
3. Fetch a hosted manifest.
4. Download Modrinth mods and external mods.
5. Verify hashes.
6. Install into a launcher-managed folder.
7. Show update/repair state.
8. Update bot wording away from CurseForge codes.

## Later Features

- Microsoft login and direct game launch.
- Auto-detect Java.
- Download Java runtime if missing.
- Optional client-only mods.
- Mod toggles.
- Pack icons/screenshots.
- Auto-generated changelog.
- Multiple server packs.
- Pelican-to-manifest full automation.
- Export zip for manual install.

## Open Questions

1. Should the first version only install files, or should it also launch Minecraft?
2. Should the manifest API live inside the existing Discord bot or as a separate service?
3. Should external mods be hosted by us, or should the launcher download from original URLs?
4. Should users be able to choose optional mods, or should the pack be strict?
5. Should we support CurseForge files at all, or avoid CurseForge completely?

## Recommended First Decision

Start with a Windows-only Tauri launcher that installs/updates the profile but does not launch Minecraft yet. That gives the fastest reliable replacement for CurseForge codes without getting stuck on Microsoft auth and launcher internals.

Once install/update works, add the Play button.
