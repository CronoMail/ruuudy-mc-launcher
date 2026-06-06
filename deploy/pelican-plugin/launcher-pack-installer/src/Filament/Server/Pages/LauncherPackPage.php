<?php

namespace Ruuudy\LauncherPackInstaller\Filament\Server\Pages;

use App\Models\Server;
use Filament\Facades\Filament;
use Filament\Notifications\Notification;
use Filament\Pages\Page;
use Ruuudy\LauncherPackInstaller\Jobs\InstallLauncherPack;
use Ruuudy\LauncherPackInstaller\Jobs\RollbackLauncherPack;
use Ruuudy\LauncherPackInstaller\Models\LauncherPackInstallation;
use Ruuudy\LauncherPackInstaller\Services\LauncherPackClient;

class LauncherPackPage extends Page
{
    protected string $view = 'launcher-pack-installer::server.launcher-pack';
    protected static ?string $navigationLabel = 'Launcher Pack';
    protected static string|\BackedEnum|null $navigationIcon = 'heroicon-o-cube';

    public string $packCode = '';
    public string $mode = 'preserve';
    public bool $startAfterInstall = true;
    public string $wipeConfirmation = '';
    public ?array $preview = null;

    public function mount(): void
    {
        $latest = $this->getLatestInstallation();
        if ($latest) {
            $this->packCode = $latest->pack_code;
        }
    }

    public function getServer(): Server
    {
        return Filament::getTenant();
    }

    public function getLatestInstallation(): ?LauncherPackInstallation
    {
        return LauncherPackInstallation::where('server_id', $this->getServer()->id)->latest()->first();
    }

    public function previewPack(): void
    {
        try {
            $this->preview = app(LauncherPackClient::class)->manifest($this->normalizedCode());
        } catch (\Throwable $exception) {
            $this->preview = null;
            Notification::make()->title('Pack unavailable')->body($exception->getMessage())->danger()->send();
        }
    }

    public function install(): void
    {
        $latest = $this->getLatestInstallation();
        if ($latest?->isRunning()) {
            Notification::make()->title('An installation is already running.')->warning()->send();
            return;
        }
        if ($this->mode === 'wipe' && strtoupper(trim($this->wipeConfirmation)) !== 'WIPE') {
            Notification::make()->title('Type WIPE to confirm a fresh install.')->warning()->send();
            return;
        }

        $this->previewPack();
        if ($this->preview === null) {
            return;
        }

        $installation = LauncherPackInstallation::create([
            'server_id' => $this->getServer()->id,
            'pack_code' => $this->normalizedCode(),
            'pack_name' => $this->preview['packName'] ?? null,
            'pack_version' => $this->preview['version'] ?? null,
            'minecraft_version' => $this->preview['minecraftVersion'] ?? null,
            'loader' => $this->preview['loader']['type'] ?? null,
            'loader_version' => $this->preview['loader']['version'] ?? null,
            'mode' => $this->mode,
            'start_after_install' => $this->startAfterInstall,
            'status' => 'pending',
        ]);
        InstallLauncherPack::dispatch($installation);
        Notification::make()->title('Launcher pack installation queued')->success()->send();
    }

    public function rollback(): void
    {
        $latest = $this->getLatestInstallation();
        if (!$latest || $latest->status !== 'completed' || empty($latest->result['rollbackPath'])) {
            Notification::make()->title('No completed launcher-pack installation can be rolled back.')->warning()->send();
            return;
        }
        RollbackLauncherPack::dispatch($latest);
        Notification::make()->title('Rollback queued')->warning()->send();
    }

    private function normalizedCode(): string
    {
        $code = strtoupper(trim($this->packCode));
        if (!preg_match('/^[A-Z0-9_-]{2,32}$/', $code)) {
            throw new \RuntimeException('Pack code may only contain letters, numbers, dashes, and underscores.');
        }
        return $code;
    }
}
