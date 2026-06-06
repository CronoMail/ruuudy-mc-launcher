<?php

namespace Ruuudy\LauncherPackInstaller\Jobs;

use App\Enums\ContainerStatus;
use App\Repositories\Daemon\DaemonServerRepository;
use App\Services\Backups\InitiateBackupService;
use Illuminate\Bus\Queueable;
use Illuminate\Contracts\Queue\ShouldQueue;
use Illuminate\Foundation\Bus\Dispatchable;
use Illuminate\Queue\InteractsWithQueue;
use Illuminate\Queue\SerializesModels;
use Ruuudy\LauncherPackInstaller\Models\LauncherPackInstallation;
use Ruuudy\LauncherPackInstaller\Services\LauncherPackClient;

class InstallLauncherPack implements ShouldQueue
{
    use Dispatchable, InteractsWithQueue, Queueable, SerializesModels;

    public int $timeout = 10800;
    public int $tries = 1;

    public function __construct(private readonly LauncherPackInstallation $installation) {}

    public function handle(
        LauncherPackClient $client,
        DaemonServerRepository $daemon,
        InitiateBackupService $backups,
    ): void {
        $server = $this->installation->server;

        try {
            $manifest = $client->manifest($this->installation->pack_code);
            $this->installation->update([
                'pack_name' => $manifest['packName'] ?? $this->installation->pack_code,
                'pack_version' => $manifest['version'] ?? null,
                'minecraft_version' => $manifest['minecraftVersion'] ?? null,
                'loader' => $manifest['loader']['type'] ?? null,
                'loader_version' => $manifest['loader']['version'] ?? null,
                'status' => 'stopping',
            ]);

            $this->stopServer($daemon->setServer($server));

            $this->installation->update(['status' => 'backing_up']);
            $backup = $backups->handle(
                $server,
                'Before launcher pack ' . $this->installation->pack_code . ' ' . now()->toDateTimeString(),
                true,
            );
            $this->installation->update(['backup_uuid' => $backup->uuid]);
            $this->waitForBackup($backup);

            $job = $client->start($server->uuid, $this->installation->pack_code, $this->installation->mode);
            $this->installation->update([
                'agent_job_id' => $job['jobId'],
                'status' => 'installing',
            ]);
            $result = $this->waitForAgent($client, $job['jobId']);

            $this->installation->update([
                'status' => 'completed',
                'progress_phase' => 'completed',
                'result' => $result['result'] ?? null,
                'installed_at' => now(),
            ]);

            if ($this->installation->start_after_install) {
                $daemon->setServer($server)->power('start')->throw();
            }
        } catch (\Throwable $exception) {
            $this->installation->update([
                'status' => 'failed',
                'error_message' => $exception->getMessage(),
            ]);
            throw $exception;
        }
    }

    private function stopServer(DaemonServerRepository $daemon): void
    {
        $details = $daemon->getDetails();
        $state = ContainerStatus::tryFrom((string) ($details['state'] ?? 'missing'));
        if ($state?->isOffline() || $state === ContainerStatus::Exited) {
            return;
        }

        $daemon->power('stop')->throw();
        $deadline = time() + 180;
        while (time() < $deadline) {
            sleep(3);
            $state = ContainerStatus::tryFrom((string) ($daemon->getDetails()['state'] ?? 'missing'));
            if ($state?->isOffline() || $state === ContainerStatus::Exited) {
                return;
            }
        }
        throw new \RuntimeException('Timed out waiting for the Minecraft server to stop.');
    }

    private function waitForBackup($backup): void
    {
        $deadline = time() + config('launcher-pack-installer.backup_timeout');
        while (time() < $deadline) {
            sleep(3);
            $backup->refresh();
            if ($backup->completed_at !== null) {
                if (!$backup->is_successful) {
                    throw new \RuntimeException('The pre-install Pelican backup failed.');
                }
                return;
            }
        }
        throw new \RuntimeException('Timed out waiting for the pre-install Pelican backup.');
    }

    private function waitForAgent(LauncherPackClient $client, string $jobId): array
    {
        $deadline = time() + config('launcher-pack-installer.install_timeout');
        while (time() < $deadline) {
            sleep(max(1, config('launcher-pack-installer.poll_seconds')));
            $job = $client->status($jobId);
            $progress = $job['progress'] ?? [];
            $this->installation->update([
                'progress_phase' => $progress['phase'] ?? $job['status'] ?? null,
                'progress_completed' => $progress['completed'] ?? 0,
                'progress_total' => $progress['total'] ?? 0,
            ]);
            if (($job['status'] ?? null) === 'completed') {
                return $job;
            }
            if (($job['status'] ?? null) === 'failed') {
                throw new \RuntimeException($job['error'] ?? 'Pack installer agent failed.');
            }
        }
        throw new \RuntimeException('Timed out waiting for the pack installer agent.');
    }

    public function failed(\Throwable $exception): void
    {
        $this->installation->update([
            'status' => 'failed',
            'error_message' => $exception->getMessage(),
        ]);
    }
}
