<?php

use Illuminate\Database\Migrations\Migration;
use Illuminate\Database\Schema\Blueprint;
use Illuminate\Support\Facades\Schema;

return new class extends Migration
{
    public function up(): void
    {
        Schema::create('launcher_pack_installations', function (Blueprint $table) {
            $table->id();
            $table->unsignedInteger('server_id');
            $table->foreign('server_id')->references('id')->on('servers')->onDelete('cascade');
            $table->string('pack_code', 32);
            $table->string('pack_name')->nullable();
            $table->string('pack_version')->nullable();
            $table->string('minecraft_version')->nullable();
            $table->string('loader')->nullable();
            $table->string('loader_version')->nullable();
            $table->string('mode', 16);
            $table->boolean('start_after_install')->default(false);
            $table->string('status', 32)->default('pending');
            $table->uuid('agent_job_id')->nullable();
            $table->uuid('backup_uuid')->nullable();
            $table->unsignedInteger('progress_completed')->default(0);
            $table->unsignedInteger('progress_total')->default(0);
            $table->string('progress_phase')->nullable();
            $table->text('error_message')->nullable();
            $table->json('result')->nullable();
            $table->timestamp('installed_at')->nullable();
            $table->timestamps();
        });
    }

    public function down(): void
    {
        Schema::dropIfExists('launcher_pack_installations');
    }
};
