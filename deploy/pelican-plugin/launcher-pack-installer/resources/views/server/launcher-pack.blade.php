<x-filament-panels::page>
    @php $latest = $this->getLatestInstallation(); @endphp
    <div class="space-y-6" wire:poll.3000ms>
        <x-filament::section heading="Launcher API pack">
            <div class="grid gap-4 lg:grid-cols-[minmax(0,1fr)_auto]">
                <x-filament::input.wrapper>
                    <x-filament::input wire:model="packCode" placeholder="Pack code, for example BIOHAZARD" />
                </x-filament::input.wrapper>
                <x-filament::button type="button" wire:click="previewPack" color="gray">Check pack</x-filament::button>
            </div>

            @if($preview)
                <div class="mt-4 grid gap-3 sm:grid-cols-2 lg:grid-cols-4 text-sm">
                    <div><div class="text-gray-500">Pack</div><div class="font-semibold">{{ $preview['packName'] }}</div></div>
                    <div><div class="text-gray-500">Version</div><div class="font-semibold">{{ $preview['version'] }}</div></div>
                    <div><div class="text-gray-500">Minecraft</div><div class="font-semibold">{{ $preview['minecraftVersion'] }}</div></div>
                    <div><div class="text-gray-500">Loader</div><div class="font-semibold">{{ ucfirst($preview['loader']['type']) }} {{ $preview['loader']['version'] }}</div></div>
                </div>
            @endif
        </x-filament::section>

        <x-filament::section heading="Install mode">
            <div class="grid gap-4 md:grid-cols-2">
                <label class="flex gap-3 rounded-lg border border-gray-300 p-4 dark:border-gray-700">
                    <input type="radio" wire:model.live="mode" value="preserve">
                    <span><strong>Preserve server data</strong><br><span class="text-sm text-gray-500">Keep worlds, server properties, operators, whitelist, bans, and loader runtime files.</span></span>
                </label>
                <label class="flex gap-3 rounded-lg border border-danger-400 p-4 dark:border-danger-700">
                    <input type="radio" wire:model.live="mode" value="wipe">
                    <span><strong>Fresh wipe</strong><br><span class="text-sm text-gray-500">Replace server content while keeping only the loader runtime and rollback data.</span></span>
                </label>
            </div>
            @if($mode === 'wipe')
                <div class="mt-4">
                    <x-filament::input.wrapper>
                        <x-filament::input wire:model="wipeConfirmation" placeholder="Type WIPE to confirm" />
                    </x-filament::input.wrapper>
                </div>
            @endif
            <label class="mt-4 flex items-center gap-2 text-sm">
                <input type="checkbox" wire:model="startAfterInstall">
                Start server after a successful installation
            </label>
            <div class="mt-4">
                <x-filament::button type="button" wire:click="install" :disabled="$latest?->isRunning()">Install or update pack</x-filament::button>
            </div>
        </x-filament::section>

        @if($latest)
            <x-filament::section heading="Latest installation">
                <div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-4 text-sm">
                    <div><div class="text-gray-500">Code</div><div class="font-semibold">{{ $latest->pack_code }}</div></div>
                    <div><div class="text-gray-500">Status</div><div class="font-semibold">{{ str_replace('_', ' ', ucfirst($latest->status)) }}</div></div>
                    <div><div class="text-gray-500">Phase</div><div class="font-semibold">{{ $latest->progress_phase ?: 'Waiting' }}</div></div>
                    <div><div class="text-gray-500">Progress</div><div class="font-semibold">{{ $latest->progress_completed }} / {{ $latest->progress_total }}</div></div>
                </div>
                @if($latest->error_message)
                    <div class="mt-4 rounded-lg border border-danger-500/50 p-3 text-sm text-danger-600">{{ $latest->error_message }}</div>
                @endif
                @if($latest->status === 'completed' && !empty($latest->result['rollbackPath']))
                    <div class="mt-4">
                        <x-filament::button type="button" wire:click="rollback" color="danger">Rollback previous server files</x-filament::button>
                    </div>
                @endif
            </x-filament::section>
        @endif
    </div>
</x-filament-panels::page>
