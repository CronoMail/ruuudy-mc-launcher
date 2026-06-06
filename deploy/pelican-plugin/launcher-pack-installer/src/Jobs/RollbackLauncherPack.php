<?php

namespace Ruuudy\LauncherPackInstaller\Jobs;

use App\Repositories\Daemon\DaemonServerRepository;
use Illuminate\Bus\Queueable;
use Illuminate\Contracts\Queue\ShouldQueue;
use Illuminate\Foundation\Bus\Dispatchable;
use Illuminate\Queue\InteractsWithQueue;
use Illuminate\Queue\SerializesModels;
use Ruuudy\LauncherPackInstaller\Models\LauncherPackInstallation;
use Ruuudy\LauncherPackInstaller\Services\LauncherPackClient;

class RollbackLauncherPack implements ShouldQueue
{
    use Dispatchable, InteractsWithQueue, Queueable, SerializesModels;

    public int $timeout = 1800;
    public int $tries = 1;

    public function __construct(private readonly LauncherPackInstallation $installation) {}

    public function handle(LauncherPackClient $client, DaemonServerRepository $daemon): void
    {
        $rollbackPath = $this->installation->result['rollbackPath'] ?? null;
        if (!$rollbackPath) {
            throw new \RuntimeException('This installation has no rollback data.');
        }
        $daemon->setServer($this->installation->server)->power('stop')->throw();
        sleep(10);
        $client->rollback($this->installation->server->uuid, $rollbackPath);
        $this->installation->update([
            'status' => 'rolled_back',
            'progress_phase' => 'rolled_back',
        ]);
    }
}
