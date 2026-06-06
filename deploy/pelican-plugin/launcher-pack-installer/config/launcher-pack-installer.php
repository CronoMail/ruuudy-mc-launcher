<?php

return [
    'pack_api_base' => rtrim((string) env('LAUNCHER_PACK_API_BASE', 'https://launcher.ruuudy.in'), '/'),
    'agent_url' => rtrim((string) env('LAUNCHER_PACK_AGENT_URL', 'http://ruuudy-pack-installer:8790'), '/'),
    'agent_token' => (string) env('LAUNCHER_PACK_AGENT_TOKEN', ''),
    'poll_seconds' => (int) env('LAUNCHER_PACK_POLL_SECONDS', 3),
    'backup_timeout' => (int) env('LAUNCHER_PACK_BACKUP_TIMEOUT', 3600),
    'install_timeout' => (int) env('LAUNCHER_PACK_INSTALL_TIMEOUT', 7200),
];
