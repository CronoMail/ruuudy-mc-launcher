<?php

namespace Ruuudy\LauncherPackInstaller\Models;

use App\Models\Server;
use Illuminate\Database\Eloquent\Model;
use Illuminate\Database\Eloquent\Relations\BelongsTo;

class LauncherPackInstallation extends Model
{
    protected $fillable = [
        'server_id',
        'pack_code',
        'pack_name',
        'pack_version',
        'minecraft_version',
        'loader',
        'loader_version',
        'mode',
        'start_after_install',
        'status',
        'agent_job_id',
        'backup_uuid',
        'progress_completed',
        'progress_total',
        'progress_phase',
        'error_message',
        'result',
        'installed_at',
    ];

    protected function casts(): array
    {
        return [
            'start_after_install' => 'bool',
            'result' => 'array',
            'installed_at' => 'datetime',
        ];
    }

    public function server(): BelongsTo
    {
        return $this->belongsTo(Server::class);
    }

    public function isRunning(): bool
    {
        return in_array($this->status, ['pending', 'stopping', 'backing_up', 'installing'], true);
    }
}
