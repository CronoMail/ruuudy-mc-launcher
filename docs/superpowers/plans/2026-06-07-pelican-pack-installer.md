# Pelican Launcher-Pack Installer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Pelican plugin and isolated installer agent that safely installs or updates a stopped Minecraft server from a launcher API pack code in preserve or fresh-wipe mode.

**Architecture:** Extend the existing schema-version-1 launcher manifest with optional server metadata and expose a filtered server-manifest endpoint. A standalone installer agent performs staged, hash-verified filesystem transactions, while a Pelican plugin supplies the authenticated UI, stop/backup/install/start workflow, and progress display.

**Tech Stack:** Node.js 22 Pack API and installer agent, Vitest, Docker Compose, Pelican plugins, PHP/Laravel/Filament, SQLite/MySQL-compatible plugin migration.

---

## File Structure

### Existing Launcher Repository

- Modify `server/manifest-server.mjs`: validate optional distribution metadata and serve normalized server manifests.
- Create `server/manifest-contract.mjs`: pure manifest normalization/filtering helpers.
- Create `server/manifest-contract.test.ts`: API contract tests.
- Modify `src-tauri/src/lib.rs`: accept optional override/server metadata without changing desktop install behavior.
- Modify `src/core/launcherCore.test.ts`: desktop compatibility regression tests.

### Installer Agent

- Create `deploy/pelican-pack-installer/package.json`: isolated agent scripts and dependencies.
- Create `deploy/pelican-pack-installer/src/server.mjs`: authenticated HTTP job API.
- Create `deploy/pelican-pack-installer/src/installer.mjs`: staging, verification, preserve/wipe, rollback, and state logic.
- Create `deploy/pelican-pack-installer/src/paths.mjs`: UUID and filesystem-boundary validation.
- Create `deploy/pelican-pack-installer/tests/installer.test.ts`: transactional installer tests.
- Create `deploy/pelican-pack-installer/Dockerfile`: minimal runtime image.
- Create `deploy/pelican-pack-installer/compose.yml`: isolated deployment with only required mounts.
- Create `deploy/pelican-pack-installer/README.md`: deployment and recovery procedure.

### Pelican Plugin

- Create `deploy/pelican-plugin/launcher-pack-installer/plugin.json`: plugin metadata and supported panel version.
- Create `deploy/pelican-plugin/launcher-pack-installer/src/LauncherPackInstallerPlugin.php`: Filament page registration.
- Create `deploy/pelican-plugin/launcher-pack-installer/src/Providers/LauncherPackInstallerProvider.php`: routes, settings, and queue bindings.
- Create `deploy/pelican-plugin/launcher-pack-installer/src/Filament/Server/Pages/LauncherPack.php`: server-area install/update UI.
- Create `deploy/pelican-plugin/launcher-pack-installer/src/Jobs/InstallLauncherPack.php`: stop, backup, agent call, and optional restart workflow.
- Create `deploy/pelican-plugin/launcher-pack-installer/src/Models/LauncherPackInstallation.php`: linked pack and job state.
- Create `deploy/pelican-plugin/launcher-pack-installer/database/migrations/2026_06_07_000000_create_launcher_pack_installations.php`: plugin persistence.
- Create `deploy/pelican-plugin/launcher-pack-installer/tests/InstallLauncherPackTest.php`: plugin workflow tests.
- Create `deploy/pelican-plugin/launcher-pack-installer/README.md`: plugin install and update instructions.

## Task 1: Lock Down Backward-Compatible Manifest Semantics

- [ ] Add failing Pack API tests asserting `overrides[].side` defaults to `both`, invalid side values are rejected, `defaultOptions` is excluded, and `serverPack.enabled` must be true before a server manifest is available.
- [ ] Add a failing desktop-launcher regression test that deserializes two otherwise-identical manifests, one with `serverPack` and override `side` fields and one without, then asserts their desktop managed-file plans are identical.
- [ ] Run `npm test` and `cargo test --manifest-path src-tauri/Cargo.toml`; confirm the new tests fail for missing server-manifest support but existing tests remain green.
- [ ] Implement optional manifest fields without changing `schemaVersion: 1`, and keep unknown/optional server metadata out of desktop install decisions.
- [ ] Run both test suites and commit:

```powershell
git add server src src-tauri docs/superpowers/specs
git commit -m "Define backward-compatible server pack metadata"
```

## Task 2: Add the Filtered Server-Manifest API

- [ ] Create pure helpers in `server/manifest-contract.mjs` that normalize side values, reject unsafe override paths, filter to `both` and `server`, exclude `defaultOptions`, and return loader plus preserve metadata.
- [ ] Add route-level tests for:
  - `GET /api/packs/EXAMPLE/server-manifest`;
  - disabled server packs returning `409`;
  - client-only and excluded entries being absent;
  - omitted override side becoming `both`;
  - public file URLs being absolute or rooted under the requested pack.
- [ ] Add the public server-manifest route to `server/manifest-server.mjs`.
- [ ] Run `npm test`; expected result is all Pack API and launcher tests passing.
- [ ] Commit:

```powershell
git add server
git commit -m "Expose filtered server pack manifests"
```

## Task 3: Build the Transactional Installer Core

- [ ] Create tests using temporary server roots for:
  - fresh wipe;
  - preserve mode;
  - removed managed files;
  - hash mismatch before mutation;
  - invalid UUID/path traversal;
  - concurrent job lock rejection;
  - rollback restoration.
- [ ] Implement strict server UUID validation and resolve targets only beneath configured `PELICAN_SERVERS_ROOT`.
- [ ] Implement downloading into a per-job staging directory and verify every declared size and SHA-256/SHA-512 before mutation.
- [ ] Implement `.ruuudy-pack-install.json` containing pack code, version, managed paths, install mode, and completion timestamp.
- [ ] Implement preserve mode by copying preserve matches into transaction storage, replacing managed content, then restoring preserved paths.
- [ ] Implement wipe mode by moving the live directory into rollback storage and promoting the staged directory.
- [ ] Run:

```powershell
npm --prefix deploy/pelican-pack-installer test
```

Expected: all transactional installer tests pass.

- [ ] Commit:

```powershell
git add deploy/pelican-pack-installer
git commit -m "Add transactional Pelican pack installer"
```

## Task 4: Add Installer-Agent Authentication and Progress

- [ ] Add failing HTTP tests for bearer authentication, one active job per server, job status polling, structured progress, cancellation before mutation, and report retrieval.
- [ ] Implement:
  - `POST /v1/installations`;
  - `GET /v1/installations/:jobId`;
  - `POST /v1/installations/:jobId/cancel`;
  - `POST /v1/installations/:jobId/rollback`.
- [ ] Require `PACK_INSTALLER_TOKEN`, redact secrets from errors, and bind only to the internal Docker network.
- [ ] Add health endpoint `GET /health`.
- [ ] Run agent tests and build the Docker image locally.
- [ ] Commit:

```powershell
git add deploy/pelican-pack-installer
git commit -m "Expose authenticated pack installer agent"
```

## Task 5: Scaffold the Pelican Plugin Against the Installed Panel Version

- [ ] On the VM, record the current Pelican panel version and generate the plugin skeleton with `php artisan p:plugin:make`; do not modify panel core files.
- [ ] Copy the generated skeleton into `deploy/pelican-plugin/launcher-pack-installer` and set `panel_version` in `plugin.json`.
- [ ] Add plugin settings for Pack API base URL, installer-agent internal URL, installer token, and default preserve patterns.
- [ ] Add migration/model fields for server UUID, pack code, installed/available version, status, job ID, mode, start-after-install, and last report.
- [ ] Run Pelican plugin tests in the panel development/container environment.
- [ ] Commit:

```powershell
git add deploy/pelican-plugin
git commit -m "Scaffold Pelican launcher pack plugin"
```

## Task 6: Implement the Pelican Install Workflow

- [ ] Add failing plugin tests proving unauthorized users cannot install packs, running servers are stopped first, backup is requested before mutation, failed backup blocks install unless an admin explicitly bypasses it, and optional restart occurs only after success.
- [ ] Implement the `InstallLauncherPack` queued job:
  1. validate server ownership/admin permission;
  2. fetch server-manifest summary;
  3. validate loader compatibility;
  4. stop and await stopped state;
  5. request Pelican backup;
  6. submit installer-agent job;
  7. poll and persist progress;
  8. optionally start after success.
- [ ] Persist actionable failure reports without exposing API or agent tokens.
- [ ] Run plugin tests.
- [ ] Commit:

```powershell
git add deploy/pelican-plugin
git commit -m "Add Pelican pack installation workflow"
```

## Task 7: Build the Pelican Server-Area UI

- [ ] Implement the `Launcher Pack` page with linked code, current/available versions, Minecraft/loader summary, preserve/wipe segmented selection, start-after-install toggle, confirmation modal, progress, logs, last report, and rollback action.
- [ ] Require explicit typed confirmation for Fresh Wipe.
- [ ] Disable repeated actions while a job is active.
- [ ] Display loader mismatch instructions before allowing installation.
- [ ] Verify the page at desktop and mobile widths and run plugin tests.
- [ ] Commit:

```powershell
git add deploy/pelican-plugin
git commit -m "Add Pelican launcher pack management UI"
```

## Task 8: Deploy Safely and Smoke Test

- [ ] Push all local launcher-repository commits before touching the VM.
- [ ] Pull `/opt/ruuudy-mc-launcher` with `git pull --ff-only`.
- [ ] Start only the new installer-agent service; verify existing StreamRelay, standalone Pack API, Pelican panel, Wings, launcher site, and Minecraft servers remain healthy.
- [ ] Install the Pelican plugin using the documented plugin installer and run its migration.
- [ ] Create a disposable Pelican Minecraft server and publish a small fixture pack containing one `both`, one `server`, one `client`, and one excluded file.
- [ ] Verify preserve mode keeps its world/config identity files and installs only server-compatible pack files.
- [ ] Verify fresh wipe removes non-pack files, preserves external Pelican backups, and can roll back.
- [ ] Verify the desktop launcher installs the same fixture pack exactly as before server metadata was added.
- [ ] Update `C:\Users\RUUUDY\.codex\workspace-context.md` with deployed paths, service names, plugin version, recovery commands, and smoke-test result.

