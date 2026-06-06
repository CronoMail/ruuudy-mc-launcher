<?php

namespace Ruuudy\LauncherPackInstaller;

use Filament\Contracts\Plugin;
use Filament\Panel;
use Ruuudy\LauncherPackInstaller\Filament\Server\Pages\LauncherPackPage;
use Ruuudy\LauncherPackInstaller\Providers\LauncherPackInstallerPluginProvider;

class LauncherPackInstallerPlugin implements Plugin
{
    public function getId(): string
    {
        return 'launcher-pack-installer';
    }

    public function register(Panel $panel): void
    {
        app()->register(LauncherPackInstallerPluginProvider::class);
        if ($panel->getId() === 'server') {
            $panel->pages([LauncherPackPage::class]);
        }
    }

    public function boot(Panel $panel): void {}
}
