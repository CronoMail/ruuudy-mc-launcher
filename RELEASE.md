# Ruuudy MC Launcher Releases

Launcher updates are separate from pack/mod updates:

- Launcher app updates are shipped through GitHub Releases and Tauri updater artifacts.
- Pack/mod updates still sync from `https://launcher.ruuudy.in`.

## One-Time GitHub Secret

This repository has the signing secret configured. If you recreate the repository or move it, add:

- `TAURI_SIGNING_PRIVATE_KEY`: contents of `%USERPROFILE%\.tauri\ruuudy-mc-launcher.key`
No signing password secret is needed because the current updater key has no password.

The public key is already embedded in `src-tauri/tauri.conf.json`.

## Release A New Launcher Version

1. Bump the version in:
   - `package.json`
   - `src-tauri/Cargo.toml`
   - `src-tauri/tauri.conf.json`
2. Commit the change.
3. Push a tag matching the version:

```powershell
git tag v0.1.1
git push origin main
git push origin v0.1.1
```

GitHub Actions will build the Windows installer, upload it to GitHub Releases, and publish
`latest.json` for the in-app updater.

Users only need to manually install the first updater-enabled build. After that, the launcher
checks for updates on startup and has a `Check for Updates` button.
