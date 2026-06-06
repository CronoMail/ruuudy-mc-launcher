<?php

namespace Ruuudy\LauncherPackInstaller\Services;

use Illuminate\Http\Client\PendingRequest;
use Illuminate\Support\Facades\Http;

class LauncherPackClient
{
    public function manifest(string $code): array
    {
        return Http::timeout(30)
            ->get(config('launcher-pack-installer.pack_api_base') . '/api/packs/' . rawurlencode(strtoupper($code)) . '/server-manifest')
            ->throw()
            ->json();
    }

    public function start(string $serverUuid, string $code, string $mode): array
    {
        return $this->agent()
            ->post(config('launcher-pack-installer.agent_url') . '/v1/installations', [
                'serverId' => $serverUuid,
                'code' => strtoupper($code),
                'mode' => $mode,
            ])
            ->throw()
            ->json();
    }

    public function status(string $jobId): array
    {
        return $this->agent()
            ->get(config('launcher-pack-installer.agent_url') . '/v1/installations/' . rawurlencode($jobId))
            ->throw()
            ->json();
    }

    public function rollback(string $serverUuid, string $rollbackPath): array
    {
        return $this->agent()
            ->post(config('launcher-pack-installer.agent_url') . '/v1/rollbacks', [
                'serverId' => $serverUuid,
                'rollbackPath' => $rollbackPath,
            ])
            ->throw()
            ->json();
    }

    private function agent(): PendingRequest
    {
        $token = config('launcher-pack-installer.agent_token');
        if ($token === '') {
            throw new \RuntimeException('LAUNCHER_PACK_AGENT_TOKEN is not configured.');
        }

        return Http::withToken($token)->timeout(30);
    }
}
