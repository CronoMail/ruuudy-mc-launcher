# Launcher Pack Installer Pelican Plugin

Adds a `Launcher Pack` page to each Pelican server. The plugin stops the server, creates a Pelican
backup, asks the isolated installer agent to install a launcher API pack, and optionally starts the
server after success.

## Panel Environment

```text
LAUNCHER_PACK_API_BASE=https://launcher.ruuudy.in
LAUNCHER_PACK_AGENT_URL=http://ruuudy-pack-installer:8790
LAUNCHER_PACK_AGENT_TOKEN=<same long random token used by the agent>
```

## Install

Copy this directory to Pelican's persistent plugin directory, then run:

```bash
php artisan p:plugin:install
php artisan migrate --force
php artisan optimize:clear
```

The plugin targets Pelican `v1.0.0-beta34` and should be rechecked after panel upgrades.

