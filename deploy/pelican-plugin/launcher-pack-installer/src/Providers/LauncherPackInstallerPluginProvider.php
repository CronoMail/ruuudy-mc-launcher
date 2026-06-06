<?php

namespace Ruuudy\LauncherPackInstaller\Providers;

use App\Enums\TablerIcon;
use App\Models\Subuser;
use Illuminate\Support\ServiceProvider;
use Ruuudy\LauncherPackInstaller\Services\LauncherPackClient;

class LauncherPackInstallerPluginProvider extends ServiceProvider
{
    public function register(): void
    {
        $this->app->singleton(LauncherPackClient::class);
    }

    public function boot(): void
    {
        Subuser::registerCustomPermissions(
            'launcher-pack-installer',
            ['install'],
            null,
            TablerIcon::Packages,
        );
    }
}
