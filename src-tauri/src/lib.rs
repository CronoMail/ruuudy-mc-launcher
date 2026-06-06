use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256, Sha512};
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    process::Command,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use sysinfo::System;
use tauri::{AppHandle, Emitter};
use thiserror::Error;
use zip::ZipArchive;

const FAKERSBOB_MANIFEST: &str = include_str!("../packs/fakersbob/manifest.json");
const LOCAL_PENDING_URL_PREFIX: &str = "local-pending://";
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Debug, Error)]
enum LauncherError {
    #[error("{0}")]
    Message(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

impl serde::Serialize for LauncherError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

type LauncherResult<T> = Result<T, LauncherError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PackManifest {
    schema_version: u8,
    pack_id: String,
    pack_name: String,
    version: String,
    minecraft_version: String,
    loader: Loader,
    server: Server,
    files: Vec<ManifestFile>,
    overrides: Vec<ManifestOverride>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    default_options: Option<ManifestOverride>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Loader {
    #[serde(rename = "type")]
    loader_type: String,
    version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Server {
    address: String,
    port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum ManifestFile {
    #[serde(rename = "modrinth")]
    #[serde(rename_all = "camelCase")]
    Modrinth {
        side: String,
        required: bool,
        project_id: String,
        version_id: String,
        filename: String,
        sha512: String,
        size: Option<u64>,
    },
    #[serde(rename = "external")]
    #[serde(rename_all = "camelCase")]
    External {
        side: String,
        required: bool,
        name: String,
        filename: String,
        url: String,
        sha256: String,
        size: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ManifestOverride {
    path: String,
    url: String,
    sha256: String,
    size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalInstallState {
    pack_id: String,
    manifest_version: String,
    managed_files: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InstallStatus {
    installed: bool,
    installed_version: Option<String>,
    latest_version: String,
    profile_dir: String,
    minecraft_profile_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InstallSummary {
    pack_id: String,
    installed_version: String,
    profile_dir: String,
    downloads: usize,
    removals: usize,
    minecraft_profile_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CurseForgeImportSummary {
    code: String,
    pack_id: String,
    pack_name: String,
    profile_dir: String,
    minecraft_version: String,
    loader_type: String,
    loader_version: String,
    curseforge_mods: usize,
    overrides: usize,
    minecraft_profile_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CurseForgeZipPreview {
    zip_path: String,
    pack_name: String,
    minecraft_version: String,
    loader_type: String,
    loader_version: String,
    required_mods: usize,
    optional_mods: usize,
    override_files: usize,
    override_mod_jars: usize,
    resource_packs: usize,
    shader_packs: usize,
    total_override_size: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PackHealthSummary {
    ok: bool,
    recommended_action: String,
    issues: Vec<PackHealthIssue>,
    total_ram_gib: f64,
    suggested_ram_gib: u64,
    java_args: String,
    expected_files: usize,
    missing_files: usize,
    size_mismatches: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PackHealthIssue {
    severity: String,
    title: String,
    detail: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct LauncherRegistry {
    profiles: Vec<RegistryProfile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegistryProfile {
    code: String,
    pack_id: String,
    pack_name: String,
    manifest_path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProfileSummary {
    code: String,
    pack_id: String,
    pack_name: String,
    version: String,
    minecraft_version: String,
    loader_type: String,
    loader_version: String,
    server: String,
    profile_dir: String,
    installed: bool,
    installed_version: Option<String>,
    local: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PublishSummary {
    code: String,
    manifest_url: String,
    uploaded_files: usize,
    manifest: PackManifest,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FolderSyncSummary {
    manifest: PackManifest,
    removed_files: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManualProfileInput {
    code: String,
    pack_name: String,
    minecraft_version: String,
    loader_type: String,
    loader_version: String,
    server_address: String,
    server_port: u16,
}

struct ImportedCurseForgeFile {
    manifest_file: ManifestFile,
    relative_path: String,
}

#[derive(Debug, Clone)]
struct DownloadItem {
    relative_path: String,
    url: Option<String>,
    filename: String,
    hash_algorithm: HashAlgorithm,
    hash: String,
    size: Option<u64>,
    source: DownloadSource,
    modrinth_version_id: Option<String>,
}

#[derive(Debug, Clone)]
enum HashAlgorithm {
    Sha256,
    Sha512,
}

#[derive(Debug, Clone)]
enum DownloadSource {
    Modrinth,
    External,
    Override,
}

#[derive(Debug, Deserialize)]
struct ModrinthVersion {
    files: Vec<ModrinthVersionFile>,
}

#[derive(Debug, Deserialize)]
struct ModrinthSearchResponse {
    hits: Vec<ModrinthSearchHit>,
}

#[derive(Debug, Deserialize)]
struct ModrinthSearchHit {
    project_id: String,
    title: String,
    description: String,
    downloads: u64,
    icon_url: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ModrinthSearchResult {
    project_id: String,
    title: String,
    description: String,
    downloads: u64,
    icon_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ModrinthProjectVersion {
    id: String,
    files: Vec<ModrinthVersionFile>,
}

#[derive(Debug, Deserialize)]
struct ModrinthVersionFile {
    filename: String,
    url: String,
    hashes: BTreeMap<String, String>,
    size: u64,
    primary: bool,
}

#[derive(Debug, Deserialize)]
struct CurseForgeManifest {
    minecraft: CurseForgeMinecraft,
    name: String,
    files: Vec<CurseForgeFile>,
    overrides: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CurseForgeMinecraft {
    version: String,
    #[serde(rename = "modLoaders")]
    mod_loaders: Vec<CurseForgeModLoader>,
}

#[derive(Debug, Deserialize)]
struct CurseForgeModLoader {
    id: String,
    primary: bool,
}

#[derive(Debug, Deserialize)]
struct CurseForgeFile {
    #[serde(rename = "projectID")]
    project_id: u64,
    #[serde(rename = "fileID")]
    file_id: u64,
    required: bool,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ProgressEvent {
    stage: String,
    message: String,
    current: usize,
    total: usize,
}

#[tauri::command]
fn lookup_pack(code: String) -> LauncherResult<PackManifest> {
    let code = normalize_pack_code(&code)?;
    if let Some(manifest) = read_local_manifest_by_code(&code)? {
        return Ok(manifest);
    }

    match code.as_str() {
        "FAKERSBOB" => Ok(serde_json::from_str(FAKERSBOB_MANIFEST)?),
        _ => Err(LauncherError::Message(format!("Unknown pack code {code}."))),
    }
}

#[tauri::command]
fn list_profiles() -> LauncherResult<Vec<ProfileSummary>> {
    let mut profiles = Vec::new();

    for profile in read_registry()?.profiles {
        if let Ok(manifest) = read_manifest_file(Path::new(&profile.manifest_path)) {
            profiles.push(profile_summary(&profile.code, &manifest, true)?);
        }
    }

    profiles.sort_by(|left, right| left.pack_name.cmp(&right.pack_name));
    profiles.dedup_by(|left, right| left.code == right.code);
    Ok(profiles)
}

#[tauri::command]
fn lookup_remote_pack(code: String, api_base: String) -> LauncherResult<PackManifest> {
    let code = normalize_pack_code(&code)?;
    let api_base = normalize_api_base(&api_base)?;
    let client = Client::builder()
        .user_agent("RuuudyMCLauncher/0.1")
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(15))
        .build()?;
    let manifest = client
        .get(format!("{api_base}/api/packs/{code}"))
        .send()?
        .error_for_status()?
        .json()?;
    Ok(manifest)
}

#[tauri::command]
fn get_install_status(manifest: PackManifest) -> LauncherResult<InstallStatus> {
    validate_manifest(&manifest)?;
    let profile_dir = profile_dir(&manifest)?;
    let state = read_install_state(&profile_dir)?;
    let installed_version = state.map(|state| state.manifest_version);

    Ok(InstallStatus {
        installed: installed_version.as_deref() == Some(manifest.version.as_str()),
        installed_version,
        latest_version: manifest.version.clone(),
        profile_dir: profile_dir.to_string_lossy().to_string(),
        minecraft_profile_id: minecraft_profile_id(&manifest),
    })
}

#[tauri::command]
async fn get_pack_health(manifest: PackManifest) -> LauncherResult<PackHealthSummary> {
    tauri::async_runtime::spawn_blocking(move || get_pack_health_blocking(manifest))
        .await
        .map_err(|err| LauncherError::Message(format!("Pack health task failed: {err}")))?
}

fn get_pack_health_blocking(manifest: PackManifest) -> LauncherResult<PackHealthSummary> {
    validate_manifest(&manifest)?;
    let profile_dir = profile_dir(&manifest)?;
    let state = read_install_state(&profile_dir)?;
    let plan = build_install_plan(&manifest, state.as_ref());
    let expected_files = plan.next_managed_files.len();
    let mut issues = Vec::new();
    let mut missing_files = 0;
    let mut size_mismatches = 0;

    if !profile_dir.exists() {
        issues.push(PackHealthIssue {
            severity: "error".to_string(),
            title: "Profile folder missing".to_string(),
            detail: profile_dir.to_string_lossy().to_string(),
        });
    }

    if state
        .as_ref()
        .is_none_or(|state| state.manifest_version != manifest.version)
    {
        issues.push(PackHealthIssue {
            severity: "warning".to_string(),
            title: "Pack needs update".to_string(),
            detail: format!(
                "Installed {} but latest is {}.",
                state
                    .as_ref()
                    .map(|state| state.manifest_version.as_str())
                    .unwrap_or("nothing"),
                manifest.version
            ),
        });
    }

    for item in [
        manifest
            .files
            .iter()
            .map(file_to_download)
            .collect::<Vec<_>>(),
        manifest
            .overrides
            .iter()
            .map(override_to_download)
            .collect(),
    ]
    .concat()
    {
        let target = profile_dir.join(safe_relative_path(&item.relative_path)?);
        if !target.exists() {
            missing_files += 1;
            issues.push(PackHealthIssue {
                severity: "error".to_string(),
                title: "Missing managed file".to_string(),
                detail: item.relative_path,
            });
            continue;
        }

        if let Some(expected_size) = item.size {
            let actual_size = fs::metadata(&target)?.len();
            if actual_size != expected_size {
                size_mismatches += 1;
                issues.push(PackHealthIssue {
                    severity: "error".to_string(),
                    title: "Managed file size changed".to_string(),
                    detail: format!(
                        "{} expected {} bytes but found {} bytes.",
                        item.relative_path, expected_size, actual_size
                    ),
                });
            }
        }
    }

    let version_id = loader_version_id(
        &manifest.loader.loader_type,
        &manifest.loader.version,
        &manifest.minecraft_version,
    )?;
    let version_json = minecraft_dir()?
        .join("versions")
        .join(&version_id)
        .join(format!("{version_id}.json"));
    if !version_json.exists() {
        issues.push(PackHealthIssue {
            severity: "error".to_string(),
            title: "Minecraft loader profile missing".to_string(),
            detail: format!("{} is not installed in the official launcher.", version_id),
        });
    }

    check_official_launcher_profile(&manifest, &profile_dir, &version_id, &mut issues)?;

    if matches!(manifest.loader.loader_type.as_str(), "forge" | "neoforge")
        && !version_json.exists()
        && !java_available()
    {
        issues.push(PackHealthIssue {
            severity: "error".to_string(),
            title: "Java not available for loader install".to_string(),
            detail: "Install Java 17+ or add java.exe to PATH, then run Install/Repair again."
                .to_string(),
        });
    }

    let (total_ram_gib, suggested_ram_gib) = recommended_client_ram();
    let has_errors = issues.iter().any(|issue| issue.severity == "error");
    let recommended_action = if state.is_none() {
        "install"
    } else if state
        .as_ref()
        .is_some_and(|state| state.manifest_version != manifest.version)
    {
        "update"
    } else if has_errors {
        "repair"
    } else {
        "play"
    }
    .to_string();

    Ok(PackHealthSummary {
        ok: !has_errors
            && state
                .as_ref()
                .is_some_and(|state| state.manifest_version == manifest.version),
        recommended_action,
        issues,
        total_ram_gib,
        suggested_ram_gib,
        java_args: default_client_java_args(),
        expected_files,
        missing_files,
        size_mismatches,
    })
}

#[tauri::command]
async fn install_pack(app: AppHandle, manifest: PackManifest) -> LauncherResult<InstallSummary> {
    tauri::async_runtime::spawn_blocking(move || install_pack_blocking(app, manifest, None))
        .await
        .map_err(|error| LauncherError::Message(format!("Install worker failed: {error}")))?
}

#[tauri::command]
async fn install_profile_pack(
    app: AppHandle,
    code: String,
    manifest: PackManifest,
) -> LauncherResult<InstallSummary> {
    let code = normalize_pack_code(&code)?;
    tauri::async_runtime::spawn_blocking(move || install_pack_blocking(app, manifest, Some(code)))
        .await
        .map_err(|error| LauncherError::Message(format!("Install worker failed: {error}")))?
}

fn install_pack_blocking(
    app: AppHandle,
    manifest: PackManifest,
    profile_code: Option<String>,
) -> LauncherResult<InstallSummary> {
    validate_manifest(&manifest)?;
    let client = Client::builder()
        .user_agent("RuuudyMCLauncher/0.1")
        .build()?;
    let profile_dir = profile_dir(&manifest)?;
    let state = read_install_state(&profile_dir)?;
    let plan = build_install_plan(&manifest, state.as_ref());
    fs::create_dir_all(&profile_dir)?;
    fs::create_dir_all(profile_dir.join(".ruuudy-launcher"))?;

    let total_steps = plan.downloads.len() + plan.removals.len() + 2;
    emit_progress(&app, "prepare", "Preparing profile folder", 0, total_steps);

    for (index, item) in plan.downloads.iter().enumerate() {
        let resolved = resolve_download(&client, item)?;
        emit_progress(
            &app,
            "download",
            &format!("Downloading {}", item.filename),
            index + 1,
            total_steps,
        );
        download_and_verify(&client, &profile_dir, &resolved)?;
    }

    for (index, relative_path) in plan.removals.iter().enumerate() {
        emit_progress(
            &app,
            "remove",
            &format!("Removing {}", relative_path),
            plan.downloads.len() + index + 1,
            total_steps,
        );
        remove_managed_file(&profile_dir, relative_path)?;
    }

    sync_resource_pack_options(&profile_dir, &manifest, state.as_ref())?;
    sync_shader_pack_options(&profile_dir, &manifest, state.as_ref())?;

    emit_progress(
        &app,
        "loader",
        &format!(
            "Installing {} launcher profile",
            loader_display_name(&manifest.loader)
        ),
        total_steps - 1,
        total_steps,
    );
    install_loader_profile(&client, &manifest)?;
    upsert_official_launcher_profile(&manifest, &profile_dir, profile_code.as_deref())?;

    let next_state = LocalInstallState {
        pack_id: manifest.pack_id.clone(),
        manifest_version: manifest.version.clone(),
        managed_files: plan.next_managed_files,
    };
    write_install_state(&profile_dir, &next_state)?;

    emit_progress(&app, "complete", "Pack is ready", total_steps, total_steps);

    Ok(InstallSummary {
        pack_id: manifest.pack_id.clone(),
        installed_version: manifest.version.clone(),
        profile_dir: profile_dir.to_string_lossy().to_string(),
        downloads: plan.downloads.len(),
        removals: plan.removals.len(),
        minecraft_profile_id: minecraft_profile_id(&manifest),
    })
}

#[tauri::command]
async fn import_curseforge_zip(
    app: AppHandle,
    zip_path: String,
) -> LauncherResult<CurseForgeImportSummary> {
    tauri::async_runtime::spawn_blocking(move || import_curseforge_zip_blocking(app, zip_path))
        .await
        .map_err(|error| LauncherError::Message(format!("Import worker failed: {error}")))?
}

#[tauri::command]
fn inspect_curseforge_zip(zip_path: String) -> LauncherResult<CurseForgeZipPreview> {
    let zip_path_buf = PathBuf::from(&zip_path);
    if !zip_path_buf.exists() {
        return Err(LauncherError::Message(format!(
            "Zip file does not exist: {}",
            zip_path_buf.display()
        )));
    }

    let file = fs::File::open(&zip_path_buf)?;
    let mut archive = ZipArchive::new(file).map_err(|error| {
        LauncherError::Message(format!("Could not read CurseForge zip: {error}"))
    })?;
    let manifest_text = read_zip_text(&mut archive, "manifest.json")?;
    let curseforge: CurseForgeManifest = serde_json::from_str(&manifest_text)?;
    let loader = curseforge_loader(&curseforge)?;
    let required_mods = curseforge.files.iter().filter(|file| file.required).count();
    let optional_mods = curseforge.files.len().saturating_sub(required_mods);
    let overrides_root = curseforge
        .overrides
        .as_deref()
        .unwrap_or("overrides")
        .trim_matches('/');
    let mut override_files = 0;
    let mut override_mod_jars = 0;
    let mut resource_packs = 0;
    let mut shader_packs = 0;
    let mut total_override_size = 0;

    for index in 0..archive.len() {
        let file = archive.by_index(index).map_err(|error| {
            LauncherError::Message(format!("Could not read zip entry {index}: {error}"))
        })?;
        if !file.is_file() {
            continue;
        }
        let name = file.name().replace('\\', "/");
        let Some(relative) = name.strip_prefix(&format!("{overrides_root}/")) else {
            continue;
        };
        override_files += 1;
        total_override_size += file.size();
        let normalized = relative.to_lowercase();
        if normalized.starts_with("mods/") && normalized.ends_with(".jar") {
            override_mod_jars += 1;
        }
        if normalized.starts_with("resourcepacks/") && normalized.ends_with(".zip") {
            resource_packs += 1;
        }
        if normalized.starts_with("shaderpacks/") && normalized.ends_with(".zip") {
            shader_packs += 1;
        }
    }

    Ok(CurseForgeZipPreview {
        zip_path,
        pack_name: if curseforge.name.trim().is_empty() {
            "Imported Pack".to_string()
        } else {
            curseforge.name.trim().to_string()
        },
        minecraft_version: curseforge.minecraft.version,
        loader_type: loader.loader_type,
        loader_version: loader.version,
        required_mods,
        optional_mods,
        override_files,
        override_mod_jars,
        resource_packs,
        shader_packs,
        total_override_size,
    })
}

fn import_curseforge_zip_blocking(
    app: AppHandle,
    zip_path: String,
) -> LauncherResult<CurseForgeImportSummary> {
    let client = Client::builder()
        .user_agent("RuuudyMCLauncher/0.1")
        .build()?;
    let zip_path = PathBuf::from(zip_path);
    if !zip_path.exists() {
        return Err(LauncherError::Message(format!(
            "Zip file does not exist: {}",
            zip_path.display()
        )));
    }

    let file = fs::File::open(&zip_path)?;
    let mut archive = ZipArchive::new(file).map_err(|error| {
        LauncherError::Message(format!("Could not read CurseForge zip: {error}"))
    })?;
    let manifest_text = read_zip_text(&mut archive, "manifest.json")?;
    let curseforge: CurseForgeManifest = serde_json::from_str(&manifest_text)?;
    let code = unique_share_code(&share_code_from_pack_name(&curseforge.name))?;
    let loader = curseforge_loader(&curseforge)?;
    let pack_id = unique_pack_id(&format!(
        "{}-{}",
        slugify_pack_id(&curseforge.name),
        code.to_lowercase()
    ))?;
    let pack_name = if curseforge.name.trim().is_empty() {
        "Imported Pack".to_string()
    } else {
        curseforge.name.trim().to_string()
    };
    let profile_dir = profile_dir_for_pack_id(&pack_id)?;
    fs::create_dir_all(&profile_dir)?;
    fs::create_dir_all(profile_dir.join("mods"))?;

    let required_curseforge_files = curseforge.files.iter().filter(|file| file.required).count();
    let total_steps = required_curseforge_files + archive.len() + 2;
    let mut managed_files = Vec::new();
    let mut locked_files = Vec::new();

    for (index, cf_file) in curseforge
        .files
        .iter()
        .filter(|file| file.required)
        .enumerate()
    {
        emit_progress(
            &app,
            "curseforge",
            &format!("Downloading CurseForge file {}", cf_file.file_id),
            index + 1,
            total_steps,
        );
        let imported = download_curseforge_file(&client, &profile_dir, cf_file)?;
        managed_files.push(imported.relative_path);
        locked_files.push(imported.manifest_file);
    }

    let overrides_root = curseforge
        .overrides
        .unwrap_or_else(|| "overrides".to_string());
    let override_files = extract_overrides(
        &app,
        &mut archive,
        &profile_dir,
        &overrides_root,
        required_curseforge_files,
        total_steps,
        &mut managed_files,
    )?;

    let manifest = PackManifest {
        schema_version: 1,
        pack_id: pack_id.clone(),
        pack_name,
        version: format!("curseforge-import-{}", unix_timestamp()),
        minecraft_version: curseforge.minecraft.version,
        loader,
        server: Server {
            address: "mc.ruuudy.in".to_string(),
            port: 25565,
        },
        files: locked_files,
        overrides: override_files,
        default_options: None,
    };

    emit_progress(
        &app,
        "loader",
        &format!(
            "Installing {} launcher profile",
            loader_display_name(&manifest.loader)
        ),
        total_steps - 1,
        total_steps,
    );
    install_loader_profile(&client, &manifest)?;
    upsert_official_launcher_profile(&manifest, &profile_dir, Some(&code))?;

    managed_files.sort();
    managed_files.dedup();
    write_install_state(
        &profile_dir,
        &LocalInstallState {
            pack_id: pack_id.clone(),
            manifest_version: manifest.version.clone(),
            managed_files,
        },
    )?;
    save_local_manifest(&code, &manifest)?;
    upsert_registry_profile(RegistryProfile {
        code: code.clone(),
        pack_id: manifest.pack_id.clone(),
        pack_name: manifest.pack_name.clone(),
        manifest_path: manifest_path_for_code(&code)?.to_string_lossy().to_string(),
    })?;

    emit_progress(
        &app,
        "complete",
        "CurseForge import complete",
        total_steps,
        total_steps,
    );

    Ok(CurseForgeImportSummary {
        code,
        pack_id,
        pack_name: manifest.pack_name.clone(),
        profile_dir: profile_dir.to_string_lossy().to_string(),
        minecraft_version: manifest.minecraft_version.clone(),
        loader_type: manifest.loader.loader_type.clone(),
        loader_version: manifest.loader.version.clone(),
        curseforge_mods: required_curseforge_files,
        overrides: manifest.overrides.len(),
        minecraft_profile_id: minecraft_profile_id(&manifest),
    })
}

#[tauri::command]
fn delete_profile(code: String) -> LauncherResult<()> {
    let code = normalize_pack_code(&code)?;
    let manifest = lookup_pack(code.clone())?;
    let profile_dir = profile_dir(&manifest)?;
    if profile_dir.exists() {
        fs::remove_dir_all(&profile_dir)?;
    }
    remove_official_launcher_profile(&manifest)?;

    let mut registry = read_registry()?;
    registry.profiles.retain(|profile| profile.code != code);
    write_registry(&registry)?;
    let manifest_path = manifest_path_for_code(&code)?;
    if manifest_path.exists() {
        fs::remove_file(manifest_path)?;
    }

    Ok(())
}

#[tauri::command]
fn export_profile_manifest(code: String) -> LauncherResult<String> {
    let code = normalize_pack_code(&code)?;
    let manifest = lookup_pack(code.clone())?;
    let downloads = dirs::download_dir()
        .or_else(|| dirs::home_dir().map(|home| home.join("Downloads")))
        .ok_or_else(|| LauncherError::Message("Could not find Downloads folder.".to_string()))?;
    fs::create_dir_all(&downloads)?;
    let path = downloads.join(format!(
        "{}-{}.ruuudypack.json",
        manifest.pack_id, manifest.version
    ));
    fs::write(&path, serde_json::to_string_pretty(&manifest)?)?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
fn save_profile_manifest(code: String, manifest: PackManifest) -> LauncherResult<ProfileSummary> {
    validate_manifest(&manifest)?;
    let code = normalize_pack_code(&code)?;
    save_local_manifest(&code, &manifest)?;
    upsert_registry_profile(RegistryProfile {
        code: code.clone(),
        pack_id: manifest.pack_id.clone(),
        pack_name: manifest.pack_name.clone(),
        manifest_path: manifest_path_for_code(&code)?.to_string_lossy().to_string(),
    })?;
    profile_summary(&code, &manifest, true)
}

#[tauri::command]
fn create_blank_profile(input: ManualProfileInput) -> LauncherResult<ProfileSummary> {
    let code = normalize_pack_code(&input.code)?;
    let loader_type = normalize_loader_type(&input.loader_type)?;
    let loader_version = input.loader_version.trim().to_string();
    if loader_type != "vanilla" && loader_version.is_empty() {
        return Err(LauncherError::Message(
            "Loader version is required for Fabric, Forge, and NeoForge profiles.".to_string(),
        ));
    }

    let pack_name = if input.pack_name.trim().is_empty() {
        code.clone()
    } else {
        input.pack_name.trim().to_string()
    };
    let pack_id = unique_pack_id(&format!(
        "{}-{}",
        slugify_pack_id(&pack_name),
        code.to_lowercase()
    ))?;
    let server_address = if input.server_address.trim().is_empty() {
        "mc.ruuudy.in".to_string()
    } else {
        input.server_address.trim().to_string()
    };
    let manifest = PackManifest {
        schema_version: 1,
        pack_id,
        pack_name,
        version: format!("manual-{}", unix_timestamp()),
        minecraft_version: input.minecraft_version.trim().to_string(),
        loader: Loader {
            loader_type,
            version: loader_version,
        },
        server: Server {
            address: server_address,
            port: input.server_port,
        },
        files: Vec::new(),
        overrides: Vec::new(),
        default_options: None,
    };
    validate_manifest(&manifest)?;
    save_profile_manifest(code, manifest)
}

fn save_published_manifest(code: &str, manifest: &PackManifest) -> LauncherResult<()> {
    save_local_manifest(code, manifest)?;
    upsert_registry_profile(RegistryProfile {
        code: code.to_string(),
        pack_id: manifest.pack_id.clone(),
        pack_name: manifest.pack_name.clone(),
        manifest_path: manifest_path_for_code(code)?.to_string_lossy().to_string(),
    })
}

#[tauri::command]
async fn sync_manifest_with_profile_folder(
    code: String,
    manifest: PackManifest,
) -> LauncherResult<FolderSyncSummary> {
    tauri::async_runtime::spawn_blocking(move || {
        sync_manifest_with_profile_folder_blocking(code, manifest)
    })
    .await
    .map_err(|error| LauncherError::Message(format!("Folder sync worker failed: {error}")))?
}

fn sync_manifest_with_profile_folder_blocking(
    code: String,
    manifest: PackManifest,
) -> LauncherResult<FolderSyncSummary> {
    let code = normalize_pack_code(&code)?;
    let summary = sync_manifest_with_profile_folder_inner(manifest)?;
    let manifest_path = manifest_path_for_code(&code)?.to_string_lossy().to_string();
    save_local_manifest(&code, &summary.manifest)?;
    upsert_registry_profile(RegistryProfile {
        code,
        pack_id: summary.manifest.pack_id.clone(),
        pack_name: summary.manifest.pack_name.clone(),
        manifest_path,
    })?;
    Ok(summary)
}

#[tauri::command]
async fn publish_profile(
    api_base: String,
    admin_token: String,
    code: String,
    manifest: PackManifest,
) -> LauncherResult<PublishSummary> {
    tauri::async_runtime::spawn_blocking(move || {
        publish_profile_blocking(api_base, admin_token, code, manifest)
    })
    .await
    .map_err(|error| LauncherError::Message(format!("Publish worker failed: {error}")))?
}

fn publish_profile_blocking(
    api_base: String,
    admin_token: String,
    code: String,
    manifest: PackManifest,
) -> LauncherResult<PublishSummary> {
    validate_manifest(&manifest)?;
    let code = normalize_pack_code(&code)?;
    let api_base = normalize_api_base(&api_base)?;
    if admin_token.trim().is_empty() {
        return Err(LauncherError::Message(
            "Admin token is required to publish a profile.".to_string(),
        ));
    }

    let client = Client::builder()
        .user_agent("RuuudyMCLauncher/0.1")
        .build()?;
    let (publish_manifest, uploaded_files) = upload_unrepresented_managed_files(
        &client,
        &api_base,
        admin_token.trim(),
        &code,
        manifest,
    )?;
    client
        .put(format!("{api_base}/api/admin/packs/{code}"))
        .bearer_auth(admin_token.trim())
        .json(&publish_manifest)
        .send()?
        .error_for_status()?;
    save_published_manifest(&code, &publish_manifest)?;

    Ok(PublishSummary {
        code: code.clone(),
        manifest_url: format!("{api_base}/api/packs/{code}"),
        uploaded_files,
        manifest: publish_manifest,
    })
}

#[tauri::command]
async fn upload_default_options(
    api_base: String,
    admin_token: String,
    code: String,
    manifest: PackManifest,
    options_path: String,
) -> LauncherResult<PublishSummary> {
    tauri::async_runtime::spawn_blocking(move || {
        upload_default_options_blocking(api_base, admin_token, code, manifest, options_path)
    })
    .await
    .map_err(|error| {
        LauncherError::Message(format!("Default keybind upload worker failed: {error}"))
    })?
}

fn upload_default_options_blocking(
    api_base: String,
    admin_token: String,
    code: String,
    manifest: PackManifest,
    options_path: String,
) -> LauncherResult<PublishSummary> {
    validate_manifest(&manifest)?;
    let code = normalize_pack_code(&code)?;
    let api_base = normalize_api_base(&api_base)?;
    if admin_token.trim().is_empty() {
        return Err(LauncherError::Message(
            "Admin token is required to upload default keybinds.".to_string(),
        ));
    }

    let source_path = PathBuf::from(options_path);
    if !source_path.exists() || !source_path.is_file() {
        return Err(LauncherError::Message(
            "Selected options.txt file does not exist.".to_string(),
        ));
    }
    let filename = source_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    if filename != "options.txt" {
        return Err(LauncherError::Message(
            "Select the Minecraft options.txt file for default keybinds.".to_string(),
        ));
    }

    let bytes = fs::read(&source_path).map_err(|err| {
        LauncherError::Message(format!(
            "Could not read selected options.txt. Close Minecraft/the launcher if it is still open, then try again. ({err})"
        ))
    })?;

    let client = Client::builder()
        .user_agent("RuuudyMCLauncher/0.1")
        .build()?;
    let (url, sha256, size) = upload_pack_file_bytes(
        &client,
        &api_base,
        admin_token.trim(),
        &code,
        "options.txt",
        bytes,
    )?;

    let mut publish_manifest = manifest;
    publish_manifest.default_options = Some(ManifestOverride {
        path: "options.txt".to_string(),
        url,
        sha256,
        size,
    });
    publish_manifest.version = format!("manual-{}", unix_timestamp());
    validate_manifest(&publish_manifest)?;

    client
        .put(format!("{api_base}/api/admin/packs/{code}"))
        .bearer_auth(admin_token.trim())
        .json(&publish_manifest)
        .send()?
        .error_for_status()?;
    save_published_manifest(&code, &publish_manifest)?;

    Ok(PublishSummary {
        code: code.clone(),
        manifest_url: format!("{api_base}/api/packs/{code}"),
        uploaded_files: 1,
        manifest: publish_manifest,
    })
}

#[tauri::command]
async fn reset_default_options(manifest: PackManifest) -> LauncherResult<()> {
    tauri::async_runtime::spawn_blocking(move || reset_default_options_blocking(manifest))
        .await
        .map_err(|error| {
            LauncherError::Message(format!("Default keybind reset worker failed: {error}"))
        })?
}

fn reset_default_options_blocking(manifest: PackManifest) -> LauncherResult<()> {
    validate_manifest(&manifest)?;
    let default_options = manifest.default_options.as_ref().ok_or_else(|| {
        LauncherError::Message("This pack has no default keybinds uploaded.".to_string())
    })?;
    let profile_dir = profile_dir(&manifest)?;
    fs::create_dir_all(&profile_dir)?;
    let client = Client::builder()
        .user_agent("RuuudyMCLauncher/0.1")
        .build()?;
    let item = override_to_download(default_options);
    let resolved = resolve_download(&client, &item)?;
    download_and_verify(&client, &profile_dir, &resolved)
}

#[tauri::command]
async fn search_modrinth_mods(
    query: String,
    minecraft_version: String,
    loader: String,
) -> LauncherResult<Vec<ModrinthSearchResult>> {
    tauri::async_runtime::spawn_blocking(move || {
        search_modrinth_mods_blocking(query, minecraft_version, loader)
    })
    .await
    .map_err(|error| LauncherError::Message(format!("Modrinth search worker failed: {error}")))?
}

fn search_modrinth_mods_blocking(
    query: String,
    minecraft_version: String,
    loader: String,
) -> LauncherResult<Vec<ModrinthSearchResult>> {
    let query = query.trim();
    if query.len() < 2 {
        return Ok(Vec::new());
    }

    let facets = format!(
        r#"[["project_type:mod"],["categories:{}"],["versions:{}"]]"#,
        loader.trim().to_lowercase(),
        minecraft_version.trim()
    );
    let client = Client::builder()
        .user_agent("RuuudyMCLauncher/0.1")
        .build()?;
    let response: ModrinthSearchResponse = client
        .get("https://api.modrinth.com/v2/search")
        .query(&[
            ("query", query),
            ("limit", "20"),
            ("index", "downloads"),
            ("facets", facets.as_str()),
        ])
        .send()?
        .error_for_status()?
        .json()?;

    Ok(response
        .hits
        .into_iter()
        .map(|hit| ModrinthSearchResult {
            project_id: hit.project_id,
            title: hit.title,
            description: hit.description,
            downloads: hit.downloads,
            icon_url: hit.icon_url,
        })
        .collect())
}

#[tauri::command]
async fn add_modrinth_mod_to_profile(
    code: String,
    manifest: PackManifest,
    project_id: String,
) -> LauncherResult<PackManifest> {
    tauri::async_runtime::spawn_blocking(move || {
        add_modrinth_mod_to_profile_blocking(code, manifest, project_id)
    })
    .await
    .map_err(|error| LauncherError::Message(format!("Modrinth add worker failed: {error}")))?
}

fn add_modrinth_mod_to_profile_blocking(
    code: String,
    manifest: PackManifest,
    project_id: String,
) -> LauncherResult<PackManifest> {
    let code = normalize_pack_code(&code)?;
    validate_manifest(&manifest)?;
    let client = Client::builder()
        .user_agent("RuuudyMCLauncher/0.1")
        .build()?;
    let mod_file = resolve_latest_modrinth_project_file(&client, &manifest, &project_id)?;
    let mut next_manifest = manifest;
    next_manifest.files.retain(|file| match file {
        ManifestFile::Modrinth {
            project_id: existing_project_id,
            ..
        } => existing_project_id != &project_id,
        ManifestFile::External { .. } => true,
    });
    next_manifest.files.push(mod_file);
    next_manifest
        .files
        .sort_by(|left, right| manifest_file_name(left).cmp(manifest_file_name(right)));
    next_manifest.version = format!("manual-{}", unix_timestamp());
    save_profile_manifest(code, next_manifest.clone())?;
    Ok(next_manifest)
}

#[tauri::command]
async fn import_local_jar_to_profile(
    code: String,
    manifest: PackManifest,
    jar_path: String,
) -> LauncherResult<PackManifest> {
    import_local_jars_to_profile(code, manifest, vec![jar_path]).await
}

#[tauri::command]
async fn import_local_jars_to_profile(
    code: String,
    manifest: PackManifest,
    jar_paths: Vec<String>,
) -> LauncherResult<PackManifest> {
    tauri::async_runtime::spawn_blocking(move || {
        import_local_jars_to_profile_blocking(code, manifest, jar_paths)
    })
    .await
    .map_err(|error| LauncherError::Message(format!("Local jar import worker failed: {error}")))?
}

#[tauri::command]
async fn import_local_resource_packs_to_profile(
    code: String,
    manifest: PackManifest,
    resource_pack_paths: Vec<String>,
) -> LauncherResult<PackManifest> {
    tauri::async_runtime::spawn_blocking(move || {
        import_local_resource_packs_to_profile_blocking(code, manifest, resource_pack_paths)
    })
    .await
    .map_err(|error| {
        LauncherError::Message(format!("Resource pack import worker failed: {error}"))
    })?
}

#[tauri::command]
async fn import_local_shader_packs_to_profile(
    code: String,
    manifest: PackManifest,
    shader_pack_paths: Vec<String>,
) -> LauncherResult<PackManifest> {
    tauri::async_runtime::spawn_blocking(move || {
        import_local_shader_packs_to_profile_blocking(code, manifest, shader_pack_paths)
    })
    .await
    .map_err(|error| LauncherError::Message(format!("Shader pack import worker failed: {error}")))?
}

fn import_local_jars_to_profile_blocking(
    code: String,
    manifest: PackManifest,
    jar_paths: Vec<String>,
) -> LauncherResult<PackManifest> {
    let code = normalize_pack_code(&code)?;
    validate_manifest(&manifest)?;
    let profile_dir = profile_dir(&manifest)?;
    let source_paths = jar_paths.into_iter().map(PathBuf::from).collect::<Vec<_>>();
    let next_manifest = add_local_jar_overrides(manifest, &profile_dir, &source_paths)?;

    let next_state = LocalInstallState {
        pack_id: next_manifest.pack_id.clone(),
        manifest_version: next_manifest.version.clone(),
        managed_files: build_install_plan(&next_manifest, None).next_managed_files,
    };
    write_install_state(&profile_dir, &next_state)?;
    save_profile_manifest(code, next_manifest.clone())?;
    Ok(next_manifest)
}

fn import_local_resource_packs_to_profile_blocking(
    code: String,
    manifest: PackManifest,
    resource_pack_paths: Vec<String>,
) -> LauncherResult<PackManifest> {
    let code = normalize_pack_code(&code)?;
    validate_manifest(&manifest)?;
    let profile_dir = profile_dir(&manifest)?;
    let previous_state = read_install_state(&profile_dir)?;
    let source_paths = resource_pack_paths
        .into_iter()
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    let mut next_manifest =
        add_local_resource_pack_overrides(manifest, &profile_dir, &source_paths)?;
    sync_resource_pack_options(&profile_dir, &next_manifest, previous_state.as_ref())?;
    next_manifest.default_options = Some(local_pending_override_for_profile_file(
        &profile_dir,
        "options.txt",
    )?);

    let next_state = LocalInstallState {
        pack_id: next_manifest.pack_id.clone(),
        manifest_version: next_manifest.version.clone(),
        managed_files: build_install_plan(&next_manifest, None).next_managed_files,
    };
    write_install_state(&profile_dir, &next_state)?;
    save_profile_manifest(code, next_manifest.clone())?;
    Ok(next_manifest)
}

fn import_local_shader_packs_to_profile_blocking(
    code: String,
    manifest: PackManifest,
    shader_pack_paths: Vec<String>,
) -> LauncherResult<PackManifest> {
    let code = normalize_pack_code(&code)?;
    validate_manifest(&manifest)?;
    let profile_dir = profile_dir(&manifest)?;
    let previous_state = read_install_state(&profile_dir)?;
    let source_paths = shader_pack_paths
        .into_iter()
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    let next_manifest = add_local_shader_pack_overrides(manifest, &profile_dir, &source_paths)?;
    sync_shader_pack_options(&profile_dir, &next_manifest, previous_state.as_ref())?;

    let next_state = LocalInstallState {
        pack_id: next_manifest.pack_id.clone(),
        manifest_version: next_manifest.version.clone(),
        managed_files: build_install_plan(&next_manifest, None).next_managed_files,
    };
    write_install_state(&profile_dir, &next_state)?;
    save_profile_manifest(code, next_manifest.clone())?;
    Ok(next_manifest)
}

fn local_pending_override_for_profile_file(
    profile_dir: &Path,
    relative_path: &str,
) -> LauncherResult<ManifestOverride> {
    let safe_path = safe_relative_path(relative_path)?;
    let path = profile_dir.join(safe_path);
    let bytes = fs::read(&path)?;
    Ok(ManifestOverride {
        path: relative_path.to_string(),
        url: format!("{LOCAL_PENDING_URL_PREFIX}{relative_path}"),
        sha256: hex::encode(Sha256::digest(&bytes)),
        size: bytes.len() as u64,
    })
}

fn add_local_jar_overrides(
    mut manifest: PackManifest,
    profile_dir: &Path,
    jar_paths: &[PathBuf],
) -> LauncherResult<PackManifest> {
    if jar_paths.is_empty() {
        return Err(LauncherError::Message(
            "Select at least one local jar to import.".to_string(),
        ));
    }

    for source_path in jar_paths {
        if !source_path.exists() || !source_path.is_file() {
            return Err(LauncherError::Message(
                "Selected jar file does not exist.".to_string(),
            ));
        }

        let filename = source_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| LauncherError::Message("Selected jar has no filename.".to_string()))?
            .to_string();
        if !filename.to_lowercase().ends_with(".jar") {
            return Err(LauncherError::Message(
                "Only .jar files can be imported as local mods.".to_string(),
            ));
        }

        let relative_path = format!("mods/{filename}");
        add_local_override_file(&mut manifest, profile_dir, source_path, &relative_path)?;
    }

    manifest
        .overrides
        .sort_by(|left, right| left.path.cmp(&right.path));
    manifest.version = format!("manual-{}", unix_timestamp());
    Ok(manifest)
}

fn add_local_resource_pack_overrides(
    mut manifest: PackManifest,
    profile_dir: &Path,
    resource_pack_paths: &[PathBuf],
) -> LauncherResult<PackManifest> {
    if resource_pack_paths.is_empty() {
        return Err(LauncherError::Message(
            "Select at least one resource pack zip to import.".to_string(),
        ));
    }

    for source_path in resource_pack_paths {
        if !source_path.exists() || !source_path.is_file() {
            return Err(LauncherError::Message(
                "Selected resource pack file does not exist.".to_string(),
            ));
        }

        let filename = source_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| {
                LauncherError::Message("Selected resource pack has no filename.".to_string())
            })?
            .to_string();
        if !filename.to_lowercase().ends_with(".zip") {
            return Err(LauncherError::Message(
                "Only .zip files can be imported as local resource packs.".to_string(),
            ));
        }

        let relative_path = format!("resourcepacks/{filename}");
        add_local_override_file(&mut manifest, profile_dir, source_path, &relative_path)?;
    }

    manifest
        .overrides
        .sort_by(|left, right| left.path.cmp(&right.path));
    manifest.version = format!("manual-{}", unix_timestamp());
    Ok(manifest)
}

fn add_local_shader_pack_overrides(
    mut manifest: PackManifest,
    profile_dir: &Path,
    shader_pack_paths: &[PathBuf],
) -> LauncherResult<PackManifest> {
    if shader_pack_paths.is_empty() {
        return Err(LauncherError::Message(
            "Select at least one shader pack zip to import.".to_string(),
        ));
    }

    for source_path in shader_pack_paths {
        if !source_path.exists() || !source_path.is_file() {
            return Err(LauncherError::Message(
                "Selected shader pack file does not exist.".to_string(),
            ));
        }

        let filename = source_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| {
                LauncherError::Message("Selected shader pack has no filename.".to_string())
            })?
            .to_string();
        if !filename.to_lowercase().ends_with(".zip") {
            return Err(LauncherError::Message(
                "Only .zip files can be imported as local shader packs.".to_string(),
            ));
        }

        let relative_path = format!("shaderpacks/{filename}");
        add_local_override_file(&mut manifest, profile_dir, source_path, &relative_path)?;
    }

    manifest
        .overrides
        .sort_by(|left, right| left.path.cmp(&right.path));
    manifest.version = format!("manual-{}", unix_timestamp());
    Ok(manifest)
}

fn add_local_override_file(
    manifest: &mut PackManifest,
    profile_dir: &Path,
    source_path: &Path,
    relative_path: &str,
) -> LauncherResult<()> {
    let target = profile_dir.join(safe_relative_path(relative_path)?);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }

    let bytes = fs::read(source_path)?;
    let sha256 = hex::encode(Sha256::digest(&bytes));
    let size = bytes.len() as u64;
    fs::write(&target, &bytes)?;

    manifest
        .files
        .retain(|file| manifest_file_relative_path(file) != relative_path);
    manifest
        .overrides
        .retain(|override_file| override_file.path.replace('\\', "/") != relative_path);
    manifest.overrides.push(ManifestOverride {
        path: relative_path.to_string(),
        url: format!("{LOCAL_PENDING_URL_PREFIX}{relative_path}"),
        sha256,
        size,
    });

    Ok(())
}

#[tauri::command]
fn open_profile_folder(manifest: PackManifest) -> LauncherResult<()> {
    let profile_dir = profile_dir(&manifest)?;
    fs::create_dir_all(&profile_dir)?;
    tauri_plugin_opener::open_path(profile_dir.to_string_lossy().to_string(), None::<&str>)
        .map_err(|error| LauncherError::Message(error.to_string()))
}

#[tauri::command]
fn open_minecraft_launcher() -> LauncherResult<()> {
    let local_app_data = std::env::var("LOCALAPPDATA")
        .map_err(|_| LauncherError::Message("LOCALAPPDATA is not set.".to_string()))?;
    let launcher = PathBuf::from(local_app_data)
        .join("Packages")
        .join("Microsoft.4297127D64EC6_8wekyb3d8bbwe")
        .join("LocalCache")
        .join("Local")
        .join("game")
        .join("Minecraft Launcher")
        .join("MinecraftLauncher.exe");

    if launcher.exists() {
        Command::new(launcher).spawn()?;
        return Ok(());
    }

    Command::new("explorer.exe")
        .arg("shell:AppsFolder\\Microsoft.4297127D64EC6_8wekyb3d8bbwe!Minecraft")
        .spawn()?;
    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            app.handle()
                .plugin(tauri_plugin_updater::Builder::new().build())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            lookup_pack,
            lookup_remote_pack,
            list_profiles,
            get_install_status,
            get_pack_health,
            install_pack,
            install_profile_pack,
            inspect_curseforge_zip,
            import_curseforge_zip,
            delete_profile,
            export_profile_manifest,
            save_profile_manifest,
            create_blank_profile,
            sync_manifest_with_profile_folder,
            publish_profile,
            upload_default_options,
            reset_default_options,
            search_modrinth_mods,
            add_modrinth_mod_to_profile,
            import_local_jar_to_profile,
            import_local_jars_to_profile,
            import_local_resource_packs_to_profile,
            import_local_shader_packs_to_profile,
            open_profile_folder,
            open_minecraft_launcher
        ])
        .run(tauri::generate_context!())
        .expect("error while running Ruuudy MC Launcher");
}

struct InstallPlan {
    downloads: Vec<DownloadItem>,
    removals: Vec<String>,
    next_managed_files: Vec<String>,
}

fn normalize_pack_code(code: &str) -> LauncherResult<String> {
    let normalized = code.trim().to_uppercase();
    let valid = normalized.len() >= 2
        && normalized.len() <= 32
        && normalized
            .chars()
            .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '-' || ch == '_');

    if valid {
        Ok(normalized)
    } else {
        Err(LauncherError::Message(
            "Pack codes may only contain letters, numbers, dashes, and underscores.".to_string(),
        ))
    }
}

fn normalize_api_base(api_base: &str) -> LauncherResult<String> {
    let trimmed = api_base.trim().trim_end_matches('/');
    if trimmed.starts_with("https://") || trimmed.starts_with("http://") {
        Ok(trimmed.to_string())
    } else {
        Err(LauncherError::Message(
            "API URL must start with http:// or https://.".to_string(),
        ))
    }
}

fn validate_manifest(manifest: &PackManifest) -> LauncherResult<()> {
    if manifest.schema_version != 1 {
        return Err(LauncherError::Message(format!(
            "Unsupported manifest schema {}.",
            manifest.schema_version
        )));
    }

    let loader_type = normalize_loader_type(&manifest.loader.loader_type)?;
    if loader_type != manifest.loader.loader_type {
        return Err(LauncherError::Message(format!(
            "Manifest loader type must be lowercase '{}'.",
            loader_type
        )));
    }
    if manifest.loader.loader_type != "vanilla" && manifest.loader.version.trim().is_empty() {
        return Err(LauncherError::Message(format!(
            "{} loader version is required.",
            loader_display_name(&manifest.loader)
        )));
    }

    for file in &manifest.files {
        match file {
            ManifestFile::Modrinth {
                filename, sha512, ..
            } => {
                if !is_hex_hash(sha512, 128) {
                    return Err(LauncherError::Message(format!(
                        "Modrinth file {filename} must include a SHA-512 hash."
                    )));
                }
            }
            ManifestFile::External {
                filename, sha256, ..
            } => {
                if !is_hex_hash(sha256, 64) {
                    return Err(LauncherError::Message(format!(
                        "External file {filename} must include a SHA-256 hash."
                    )));
                }
            }
        }
    }

    for override_file in &manifest.overrides {
        if !is_hex_hash(&override_file.sha256, 64) {
            return Err(LauncherError::Message(format!(
                "Override {} must include a SHA-256 hash.",
                override_file.path
            )));
        }
    }

    if let Some(default_options) = &manifest.default_options {
        if default_options.path.replace('\\', "/") != "options.txt" {
            return Err(LauncherError::Message(
                "Default options must target options.txt.".to_string(),
            ));
        }

        if !is_hex_hash(&default_options.sha256, 64) {
            return Err(LauncherError::Message(
                "Default options must include a SHA-256 hash.".to_string(),
            ));
        }
    }

    Ok(())
}

fn normalize_loader_type(loader_type: &str) -> LauncherResult<String> {
    let normalized = loader_type.trim().to_ascii_lowercase().replace('_', "-");
    match normalized.as_str() {
        "vanilla" | "fabric" | "forge" => Ok(normalized),
        "neoforge" | "neo-forge" => Ok("neoforge".to_string()),
        _ => Err(LauncherError::Message(format!(
            "Unsupported loader '{}'. Supported loaders are Vanilla, Fabric, Forge, and NeoForge.",
            loader_type
        ))),
    }
}

fn is_hex_hash(value: &str, expected_len: usize) -> bool {
    value.len() == expected_len && value.chars().all(|ch| ch.is_ascii_hexdigit())
}

fn build_install_plan(manifest: &PackManifest, state: Option<&LocalInstallState>) -> InstallPlan {
    let mut downloads: Vec<DownloadItem> = manifest
        .files
        .iter()
        .map(file_to_download)
        .chain(manifest.overrides.iter().map(override_to_download))
        .collect();

    let next_managed_files: Vec<String> = downloads
        .iter()
        .map(|item| item.relative_path.clone())
        .collect();

    if state.is_none() {
        if let Some(default_options) = &manifest.default_options {
            downloads.push(override_to_download(default_options));
        }
    }

    let next_set: BTreeSet<String> = next_managed_files.iter().cloned().collect();
    let removals = state
        .map(|state| {
            state
                .managed_files
                .iter()
                .filter(|path| !next_set.contains(*path))
                .cloned()
                .collect()
        })
        .unwrap_or_default();

    InstallPlan {
        downloads,
        removals,
        next_managed_files,
    }
}

fn file_to_download(file: &ManifestFile) -> DownloadItem {
    match file {
        ManifestFile::Modrinth {
            version_id,
            filename,
            sha512,
            size,
            ..
        } => DownloadItem {
            relative_path: format!("mods/{filename}"),
            url: None,
            filename: filename.clone(),
            hash_algorithm: HashAlgorithm::Sha512,
            hash: sha512.clone(),
            size: *size,
            source: DownloadSource::Modrinth,
            modrinth_version_id: Some(version_id.clone()),
        },
        ManifestFile::External {
            filename,
            url,
            sha256,
            size,
            ..
        } => DownloadItem {
            relative_path: format!("mods/{filename}"),
            url: Some(url.clone()),
            filename: filename.clone(),
            hash_algorithm: HashAlgorithm::Sha256,
            hash: sha256.clone(),
            size: Some(*size),
            source: DownloadSource::External,
            modrinth_version_id: None,
        },
    }
}

fn override_to_download(override_file: &ManifestOverride) -> DownloadItem {
    DownloadItem {
        relative_path: override_file.path.replace('\\', "/"),
        url: Some(override_file.url.clone()),
        filename: Path::new(&override_file.path)
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| override_file.path.clone()),
        hash_algorithm: HashAlgorithm::Sha256,
        hash: override_file.sha256.clone(),
        size: Some(override_file.size),
        source: DownloadSource::Override,
        modrinth_version_id: None,
    }
}

fn resolve_download(client: &Client, item: &DownloadItem) -> LauncherResult<DownloadItem> {
    match item.source {
        DownloadSource::Modrinth => {
            let version_id = item.modrinth_version_id.as_ref().ok_or_else(|| {
                LauncherError::Message("Missing Modrinth version id.".to_string())
            })?;
            let version: ModrinthVersion = client
                .get(format!("https://api.modrinth.com/v2/version/{version_id}"))
                .send()?
                .error_for_status()?
                .json()?;
            let file = version
                .files
                .iter()
                .find(|file| file.filename == item.filename)
                .or_else(|| version.files.iter().find(|file| file.primary))
                .ok_or_else(|| {
                    LauncherError::Message(format!(
                        "No downloadable Modrinth file found for {}.",
                        item.filename
                    ))
                })?;
            let sha512 = file.hashes.get("sha512").ok_or_else(|| {
                LauncherError::Message(format!(
                    "Modrinth file {} has no SHA-512 hash.",
                    file.filename
                ))
            })?;
            if !sha512.eq_ignore_ascii_case(&item.hash) {
                return Err(LauncherError::Message(format!(
                    "Modrinth hash changed for {}. Refusing to install.",
                    file.filename
                )));
            }

            Ok(DownloadItem {
                url: Some(file.url.clone()),
                size: Some(file.size),
                ..item.clone()
            })
        }
        DownloadSource::External | DownloadSource::Override => Ok(item.clone()),
    }
}

fn resolve_latest_modrinth_project_file(
    client: &Client,
    manifest: &PackManifest,
    project_id: &str,
) -> LauncherResult<ManifestFile> {
    let loader = manifest.loader.loader_type.as_str();
    let game_version = manifest.minecraft_version.as_str();
    let loaders_query = format!(r#"["{loader}"]"#);
    let game_versions_query = format!(r#"["{game_version}"]"#);
    let versions: Vec<ModrinthProjectVersion> = client
        .get(format!(
            "https://api.modrinth.com/v2/project/{}/version",
            project_id.trim()
        ))
        .query(&[
            ("loaders", loaders_query.as_str()),
            ("game_versions", game_versions_query.as_str()),
        ])
        .send()?
        .error_for_status()?
        .json()?;

    let version = versions.into_iter().next().ok_or_else(|| {
        LauncherError::Message(format!(
            "No {loader} version for Minecraft {game_version} was found on Modrinth."
        ))
    })?;
    let file = version
        .files
        .iter()
        .find(|file| file.primary)
        .or_else(|| version.files.first())
        .ok_or_else(|| LauncherError::Message("Modrinth version has no files.".to_string()))?;
    let sha512 = file.hashes.get("sha512").ok_or_else(|| {
        LauncherError::Message(format!(
            "Modrinth file {} has no SHA-512 hash.",
            file.filename
        ))
    })?;

    Ok(ManifestFile::Modrinth {
        side: "both".to_string(),
        required: true,
        project_id: project_id.trim().to_string(),
        version_id: version.id,
        filename: file.filename.clone(),
        sha512: sha512.clone(),
        size: Some(file.size),
    })
}

fn manifest_file_name(file: &ManifestFile) -> &str {
    match file {
        ManifestFile::Modrinth { filename, .. } => filename,
        ManifestFile::External { filename, .. } => filename,
    }
}

fn manifest_file_relative_path(file: &ManifestFile) -> String {
    format!("mods/{}", manifest_file_name(file))
}

fn sync_manifest_with_profile_folder_inner(
    manifest: PackManifest,
) -> LauncherResult<FolderSyncSummary> {
    validate_manifest(&manifest)?;
    let profile_dir = profile_dir(&manifest)?;
    let Some(state) = read_install_state(&profile_dir)? else {
        return Ok(FolderSyncSummary {
            manifest,
            removed_files: Vec::new(),
        });
    };

    let managed_set: BTreeSet<String> = state.managed_files.into_iter().collect();
    let mut removed_files = Vec::new();
    let mut synced = manifest.clone();
    synced.files.retain(|file| {
        let relative_path = manifest_file_relative_path(file);
        let Ok(safe_path) = safe_relative_path(&relative_path) else {
            removed_files.push(manifest_file_name(file).to_string());
            return false;
        };
        let missing_managed_file =
            managed_set.contains(&relative_path) && !profile_dir.join(safe_path).exists();

        if missing_managed_file {
            removed_files.push(manifest_file_name(file).to_string());
            false
        } else {
            true
        }
    });
    synced.overrides.retain(|override_file| {
        let relative_path = override_file.path.replace('\\', "/");
        let Ok(safe_path) = safe_relative_path(&relative_path) else {
            removed_files.push(override_file.path.clone());
            return false;
        };
        let missing_managed_file =
            managed_set.contains(&relative_path) && !profile_dir.join(safe_path).exists();

        if missing_managed_file {
            removed_files.push(override_file.path.clone());
            false
        } else {
            true
        }
    });

    if !removed_files.is_empty() {
        synced.version = format!("manual-{}", unix_timestamp());
        let next_state = LocalInstallState {
            pack_id: synced.pack_id.clone(),
            manifest_version: synced.version.clone(),
            managed_files: build_install_plan(&synced, None).next_managed_files,
        };
        write_install_state(&profile_dir, &next_state)?;
    }

    Ok(FolderSyncSummary {
        manifest: synced,
        removed_files,
    })
}

fn upload_unrepresented_managed_files(
    client: &Client,
    api_base: &str,
    admin_token: &str,
    code: &str,
    manifest: PackManifest,
) -> LauncherResult<(PackManifest, usize)> {
    let profile_dir = profile_dir(&manifest)?;
    let Some(state) = read_install_state(&profile_dir)? else {
        return Ok((manifest, 0));
    };

    let mut publish_manifest = manifest;
    let mut uploaded_files = 0;
    publish_manifest
        .overrides
        .retain(|override_file| is_managed_override_file(&override_file.path));

    if publish_manifest
        .default_options
        .as_ref()
        .is_some_and(|options| options.url.starts_with(LOCAL_PENDING_URL_PREFIX))
    {
        let (url, sha256, size) = upload_profile_file(
            client,
            api_base,
            admin_token,
            code,
            &profile_dir,
            "options.txt",
        )?;
        publish_manifest.default_options = Some(ManifestOverride {
            path: "options.txt".to_string(),
            url,
            sha256,
            size,
        });
        uploaded_files += 1;
    }

    for override_file in publish_manifest.overrides.iter_mut() {
        if !override_file.url.starts_with(LOCAL_PENDING_URL_PREFIX) {
            continue;
        }

        let relative_path = override_file.path.replace('\\', "/");
        let (url, sha256, size) = upload_profile_file(
            client,
            api_base,
            admin_token,
            code,
            &profile_dir,
            &relative_path,
        )?;
        override_file.url = url;
        override_file.sha256 = sha256;
        override_file.size = size;
        uploaded_files += 1;
    }

    let mut represented: BTreeSet<String> = publish_manifest
        .files
        .iter()
        .map(manifest_file_relative_path)
        .collect();
    represented.extend(
        publish_manifest
            .overrides
            .iter()
            .map(|override_file| override_file.path.replace('\\', "/")),
    );

    for relative_path in state.managed_files {
        let relative_path = relative_path.replace('\\', "/");
        if represented.contains(&relative_path) {
            continue;
        }
        if !is_managed_override_file(&relative_path) {
            continue;
        }

        if !profile_dir
            .join(safe_relative_path(&relative_path)?)
            .is_file()
        {
            continue;
        }

        let (url, sha256, size) = upload_profile_file(
            client,
            api_base,
            admin_token,
            code,
            &profile_dir,
            &relative_path,
        )?;
        publish_manifest.overrides.push(ManifestOverride {
            path: relative_path.clone(),
            url,
            sha256,
            size,
        });
        represented.insert(relative_path);
        uploaded_files += 1;
    }

    if uploaded_files > 0 {
        publish_manifest.version = format!("manual-{}", unix_timestamp());
    }

    Ok((publish_manifest, uploaded_files))
}

fn upload_profile_file(
    client: &Client,
    api_base: &str,
    admin_token: &str,
    code: &str,
    profile_dir: &Path,
    relative_path: &str,
) -> LauncherResult<(String, String, u64)> {
    let safe_path = safe_relative_path(relative_path)?;
    let source_path = profile_dir.join(safe_path);
    let bytes = fs::read(source_path)?;
    upload_pack_file_bytes(client, api_base, admin_token, code, relative_path, bytes)
}

fn upload_pack_file_bytes(
    client: &Client,
    api_base: &str,
    admin_token: &str,
    code: &str,
    relative_path: &str,
    bytes: Vec<u8>,
) -> LauncherResult<(String, String, u64)> {
    let sha256 = hex::encode(Sha256::digest(&bytes));
    let size = bytes.len() as u64;
    let file_url_path = encode_relative_url_path(relative_path);
    client
        .put(format!(
            "{api_base}/api/admin/packs/{code}/files/{file_url_path}"
        ))
        .bearer_auth(admin_token)
        .body(bytes)
        .send()?
        .error_for_status()?;

    Ok((
        format!("{api_base}/api/packs/{code}/files/{file_url_path}"),
        sha256,
        size,
    ))
}

fn encode_relative_url_path(relative_path: &str) -> String {
    relative_path
        .replace('\\', "/")
        .split('/')
        .filter(|part| !part.is_empty())
        .map(|part| urlencoding::encode(part).into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

fn download_and_verify(
    client: &Client,
    profile_dir: &Path,
    item: &DownloadItem,
) -> LauncherResult<()> {
    let relative_path = safe_relative_path(&item.relative_path)?;
    let target = profile_dir.join(relative_path);
    if target.exists() && target.is_file() {
        let bytes = fs::read(&target)?;
        if verify_hash(&bytes, item).is_ok() {
            return Ok(());
        }
    }

    let url = item
        .url
        .as_ref()
        .ok_or_else(|| LauncherError::Message(format!("No download URL for {}.", item.filename)))?;
    if url.starts_with(LOCAL_PENDING_URL_PREFIX) {
        return Err(LauncherError::Message(format!(
            "{} is a local pending override. Publish this pack before another PC can download it.",
            item.filename
        )));
    }
    let parent = target
        .parent()
        .ok_or_else(|| LauncherError::Message("Download target has no parent.".to_string()))?;
    fs::create_dir_all(parent)?;

    let mut response = client.get(url).send()?.error_for_status()?;
    let temp = target.with_extension("download");
    let mut file = fs::File::create(&temp)?;
    let mut bytes = Vec::new();
    response.read_to_end(&mut bytes)?;
    if let Some(expected_size) = item.size {
        if bytes.len() as u64 != expected_size {
            let _ = fs::remove_file(&temp);
            return Err(LauncherError::Message(format!(
                "{} size mismatch. Expected {}, got {}.",
                item.filename,
                expected_size,
                bytes.len()
            )));
        }
    }
    verify_hash(&bytes, item)?;
    file.write_all(&bytes)?;
    drop(file);
    fs::rename(temp, target)?;
    Ok(())
}

fn verify_hash(bytes: &[u8], item: &DownloadItem) -> LauncherResult<()> {
    let actual = match item.hash_algorithm {
        HashAlgorithm::Sha256 => hex::encode(Sha256::digest(bytes)),
        HashAlgorithm::Sha512 => hex::encode(Sha512::digest(bytes)),
    };
    if actual.eq_ignore_ascii_case(&item.hash) {
        Ok(())
    } else {
        Err(LauncherError::Message(format!(
            "{} hash mismatch. Refusing to install.",
            item.filename
        )))
    }
}

fn remove_managed_file(profile_dir: &Path, relative_path: &str) -> LauncherResult<()> {
    let safe_path = safe_relative_path(relative_path)?;
    let target = profile_dir.join(safe_path);
    if target.exists() && target.is_file() {
        fs::remove_file(target)?;
    }
    Ok(())
}

fn safe_relative_path(path: &str) -> LauncherResult<PathBuf> {
    let path = Path::new(path);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::Prefix(_) | Component::RootDir
            )
        })
    {
        return Err(LauncherError::Message(format!(
            "Unsafe managed path {}.",
            path.display()
        )));
    }
    Ok(path.to_path_buf())
}

fn install_loader_profile(client: &Client, manifest: &PackManifest) -> LauncherResult<()> {
    match manifest.loader.loader_type.as_str() {
        "fabric" => install_fabric_profile(client, manifest),
        "forge" => install_forge_profile(client, manifest),
        "neoforge" => install_neoforge_profile(client, manifest),
        "vanilla" => Ok(()),
        loader => Err(LauncherError::Message(format!(
            "Unsupported loader '{}'.",
            loader
        ))),
    }
}

fn install_fabric_profile(client: &Client, manifest: &PackManifest) -> LauncherResult<()> {
    let minecraft_dir = minecraft_dir()?;
    let version_id = loader_version_id(
        &manifest.loader.loader_type,
        &manifest.loader.version,
        &manifest.minecraft_version,
    )?;
    let version_dir = minecraft_dir.join("versions").join(&version_id);
    let version_json = version_dir.join(format!("{version_id}.json"));
    if version_json.exists() {
        return Ok(());
    }

    fs::create_dir_all(&version_dir)?;
    let profile_json = client
        .get(format!(
            "https://meta.fabricmc.net/v2/versions/loader/{}/{}/profile/json",
            manifest.minecraft_version, manifest.loader.version
        ))
        .send()?
        .error_for_status()?
        .text()?;
    fs::write(version_json, profile_json)?;
    Ok(())
}

fn install_forge_profile(client: &Client, manifest: &PackManifest) -> LauncherResult<()> {
    install_java_mod_loader_profile(
        client,
        manifest,
        &forge_installer_url(&manifest.minecraft_version, &manifest.loader.version),
    )
}

fn install_neoforge_profile(client: &Client, manifest: &PackManifest) -> LauncherResult<()> {
    let candidates = neoforge_installer_urls(&manifest.minecraft_version, &manifest.loader.version);
    let mut last_error = None;
    for url in candidates {
        match install_java_mod_loader_profile(client, manifest, &url) {
            Ok(()) => return Ok(()),
            Err(error) => last_error = Some(error.to_string()),
        }
    }
    Err(LauncherError::Message(format!(
        "Could not install NeoForge {} for Minecraft {}. {}",
        manifest.loader.version,
        manifest.minecraft_version,
        last_error.unwrap_or_else(|| "No installer URL was available.".to_string())
    )))
}

fn install_java_mod_loader_profile(
    client: &Client,
    manifest: &PackManifest,
    installer_url: &str,
) -> LauncherResult<()> {
    let minecraft_dir = minecraft_dir()?;
    let version_id = loader_version_id(
        &manifest.loader.loader_type,
        &manifest.loader.version,
        &manifest.minecraft_version,
    )?;
    let version_json = minecraft_dir
        .join("versions")
        .join(&version_id)
        .join(format!("{version_id}.json"));
    if version_json.exists() {
        return Ok(());
    }

    fs::create_dir_all(&minecraft_dir)?;
    let installer_dir = std::env::temp_dir().join(format!(
        "ruuudy-mc-loader-installer-{}-{}",
        std::process::id(),
        unix_timestamp()
    ));
    fs::create_dir_all(&installer_dir)?;
    let installer_path = installer_dir.join("installer.jar");

    let install_result = (|| -> LauncherResult<()> {
        let bytes = client
            .get(installer_url)
            .send()?
            .error_for_status()?
            .bytes()?;
        fs::write(&installer_path, bytes.as_ref())?;

        let mut last_error = None;
        for attempt in 1..=2 {
            match run_java_mod_loader_installer(&installer_path, &minecraft_dir) {
                Ok(()) => {
                    last_error = None;
                    break;
                }
                Err(error) => {
                    last_error = Some(error);
                    if attempt == 1 {
                        std::thread::sleep(std::time::Duration::from_secs(2));
                    }
                }
            }
        }
        if let Some(error) = last_error {
            return Err(LauncherError::Message(format!(
                "Mod loader installer failed for {}. {}",
                installer_url, error
            )));
        }
        if !version_json.exists() {
            return Err(LauncherError::Message(format!(
                "Mod loader installer finished, but {} was not created.",
                version_json.display()
            )));
        }
        Ok(())
    })();

    let _ = fs::remove_dir_all(&installer_dir);
    install_result
}

fn run_java_mod_loader_installer(
    installer_path: &Path,
    minecraft_dir: &Path,
) -> LauncherResult<()> {
    let mut command = Command::new("java");
    command
        .arg("-jar")
        .arg(installer_path)
        .arg("--installClient")
        .current_dir(minecraft_dir);
    hide_subprocess_window(&mut command);
    let output = command.output().map_err(|error| {
        LauncherError::Message(format!(
            "Could not run the Java mod loader installer. Install Java 17+ or add java.exe to PATH, then import again. {error}"
        ))
    })?;
    if output.status.success() {
        return Ok(());
    }

    Err(LauncherError::Message(format!(
        "stdout: {} stderr: {}",
        truncate_installer_output(String::from_utf8_lossy(&output.stdout).trim()),
        truncate_installer_output(String::from_utf8_lossy(&output.stderr).trim())
    )))
}

fn truncate_installer_output(output: &str) -> String {
    const MAX_CHARS: usize = 4000;
    if output.chars().count() <= MAX_CHARS {
        return output.to_string();
    }

    let tail = output
        .chars()
        .rev()
        .take(MAX_CHARS)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    format!("...{tail}")
}

fn upsert_official_launcher_profile(
    manifest: &PackManifest,
    profile_dir: &Path,
    profile_code: Option<&str>,
) -> LauncherResult<()> {
    let launcher_profiles = minecraft_dir()?.join("launcher_profiles.json");
    let mut root: serde_json::Value = if launcher_profiles.exists() {
        serde_json::from_str(&fs::read_to_string(&launcher_profiles)?)?
    } else {
        serde_json::json!({
            "profiles": {},
            "settings": {}
        })
    };

    let profile_id = minecraft_profile_id(manifest);
    let display_name = profile_code
        .map(|code| format!("{} Server", code.trim().to_uppercase()))
        .unwrap_or_else(|| format!("{} Server", manifest.pack_name));
    let now = iso_timestamp();
    let profile = serde_json::json!({
        "name": display_name,
        "type": "custom",
        "created": now,
        "lastUsed": now,
        "gameDir": profile_dir.to_string_lossy(),
        "lastVersionId": loader_version_id(
            &manifest.loader.loader_type,
            &manifest.loader.version,
            &manifest.minecraft_version,
        )?,
        "javaArgs": default_client_java_args()
    });

    if !root
        .get("profiles")
        .is_some_and(|profiles| profiles.is_object())
    {
        root["profiles"] = serde_json::json!({});
    }
    root["profiles"][profile_id.as_str()] = profile;
    root["selectedProfile"] = serde_json::Value::String(profile_id);

    if let Some(parent) = launcher_profiles.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(launcher_profiles, serde_json::to_string_pretty(&root)?)?;
    Ok(())
}

fn check_official_launcher_profile(
    manifest: &PackManifest,
    profile_dir: &Path,
    version_id: &str,
    issues: &mut Vec<PackHealthIssue>,
) -> LauncherResult<()> {
    let launcher_profiles = minecraft_dir()?.join("launcher_profiles.json");
    if !launcher_profiles.exists() {
        issues.push(PackHealthIssue {
            severity: "error".to_string(),
            title: "Minecraft launcher profile missing".to_string(),
            detail: "The official Minecraft Launcher has no launcher_profiles.json yet."
                .to_string(),
        });
        return Ok(());
    }

    let root: serde_json::Value = serde_json::from_str(&fs::read_to_string(&launcher_profiles)?)?;
    let profile_id = minecraft_profile_id(manifest);
    let Some(profile) = root
        .get("profiles")
        .and_then(|profiles| profiles.get(&profile_id))
    else {
        issues.push(PackHealthIssue {
            severity: "error".to_string(),
            title: "Minecraft launcher profile missing".to_string(),
            detail: format!("{profile_id} is not in launcher_profiles.json."),
        });
        return Ok(());
    };

    if profile
        .get("lastVersionId")
        .and_then(|value| value.as_str())
        != Some(version_id)
    {
        issues.push(PackHealthIssue {
            severity: "error".to_string(),
            title: "Minecraft launcher profile uses the wrong loader".to_string(),
            detail: format!("Expected lastVersionId {version_id}."),
        });
    }

    if profile.get("gameDir").and_then(|value| value.as_str())
        != Some(profile_dir.to_string_lossy().as_ref())
    {
        issues.push(PackHealthIssue {
            severity: "error".to_string(),
            title: "Minecraft launcher profile uses the wrong folder".to_string(),
            detail: profile_dir.to_string_lossy().to_string(),
        });
    }

    let expected_args = default_client_java_args();
    if profile.get("javaArgs").and_then(|value| value.as_str()) != Some(expected_args.as_str()) {
        issues.push(PackHealthIssue {
            severity: "info".to_string(),
            title: "Smart RAM will apply on next install/repair".to_string(),
            detail: expected_args,
        });
    }

    Ok(())
}

fn default_client_java_args() -> String {
    let (_, xmx_gib) = recommended_client_ram();
    format!("-Xmx{xmx_gib}G -XX:+UseG1GC")
}

fn recommended_client_ram() -> (f64, u64) {
    let mut system = System::new();
    system.refresh_memory();

    let total_gib = system.total_memory() as f64 / 1024.0 / 1024.0 / 1024.0;
    let xmx_gib = if total_gib >= 32.0 {
        12
    } else if total_gib >= 24.0 {
        10
    } else if total_gib >= 16.0 {
        8
    } else if total_gib >= 12.0 {
        6
    } else if total_gib >= 8.0 {
        4
    } else {
        3
    };

    (round_one_decimal(total_gib), xmx_gib)
}

fn round_one_decimal(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

fn java_available() -> bool {
    let mut command = Command::new("java");
    command.arg("-version");
    hide_subprocess_window(&mut command);
    command
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn hide_subprocess_window(command: &mut Command) {
    #[cfg(windows)]
    {
        command.creation_flags(CREATE_NO_WINDOW);
    }
}

fn remove_official_launcher_profile(manifest: &PackManifest) -> LauncherResult<()> {
    let launcher_profiles = minecraft_dir()?.join("launcher_profiles.json");
    if !launcher_profiles.exists() {
        return Ok(());
    }

    let mut root: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&launcher_profiles)?)?;
    if let Some(profiles) = root
        .get_mut("profiles")
        .and_then(|profiles| profiles.as_object_mut())
    {
        profiles.remove(&minecraft_profile_id(manifest));
    }
    fs::write(launcher_profiles, serde_json::to_string_pretty(&root)?)?;
    Ok(())
}

fn read_install_state(profile_dir: &Path) -> LauncherResult<Option<LocalInstallState>> {
    let path = install_state_path(profile_dir);
    if !path.exists() {
        return Ok(None);
    }
    Ok(Some(serde_json::from_str(&fs::read_to_string(path)?)?))
}

fn write_install_state(profile_dir: &Path, state: &LocalInstallState) -> LauncherResult<()> {
    let path = install_state_path(profile_dir);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(state)?)?;
    Ok(())
}

fn profile_summary(
    code: &str,
    manifest: &PackManifest,
    local: bool,
) -> LauncherResult<ProfileSummary> {
    let profile_dir = profile_dir(manifest)?;
    let installed_version = read_install_state(&profile_dir)?.map(|state| state.manifest_version);
    Ok(ProfileSummary {
        code: code.to_string(),
        pack_id: manifest.pack_id.clone(),
        pack_name: manifest.pack_name.clone(),
        version: manifest.version.clone(),
        minecraft_version: manifest.minecraft_version.clone(),
        loader_type: manifest.loader.loader_type.clone(),
        loader_version: manifest.loader.version.clone(),
        server: format!("{}:{}", manifest.server.address, manifest.server.port),
        profile_dir: profile_dir.to_string_lossy().to_string(),
        installed: installed_version.as_deref() == Some(manifest.version.as_str()),
        installed_version,
        local,
    })
}

fn read_local_manifest_by_code(code: &str) -> LauncherResult<Option<PackManifest>> {
    let registry = read_registry()?;
    let Some(profile) = registry
        .profiles
        .iter()
        .find(|profile| profile.code == code)
    else {
        return Ok(None);
    };
    Ok(Some(read_manifest_file(Path::new(&profile.manifest_path))?))
}

fn read_manifest_file(path: &Path) -> LauncherResult<PackManifest> {
    Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
}

fn save_local_manifest(code: &str, manifest: &PackManifest) -> LauncherResult<()> {
    let path = manifest_path_for_code(code)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(manifest)?)?;
    Ok(())
}

fn upsert_registry_profile(profile: RegistryProfile) -> LauncherResult<()> {
    let mut registry = read_registry()?;
    registry
        .profiles
        .retain(|existing| existing.code != profile.code);
    registry.profiles.push(profile);
    write_registry(&registry)
}

fn read_registry() -> LauncherResult<LauncherRegistry> {
    let path = registry_path()?;
    if !path.exists() {
        return Ok(LauncherRegistry::default());
    }
    Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
}

fn write_registry(registry: &LauncherRegistry) -> LauncherResult<()> {
    let path = registry_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(registry)?)?;
    Ok(())
}

fn registry_path() -> LauncherResult<PathBuf> {
    Ok(launcher_data_dir()?.join("launcher-registry.json"))
}

fn manifest_path_for_code(code: &str) -> LauncherResult<PathBuf> {
    Ok(launcher_data_dir()?
        .join("manifests")
        .join(format!("{code}.json")))
}

fn launcher_data_dir() -> LauncherResult<PathBuf> {
    let app_data = std::env::var("APPDATA")
        .map_err(|_| LauncherError::Message("APPDATA is not set.".to_string()))?;
    Ok(PathBuf::from(app_data).join(".ruuudy-mc"))
}

fn install_state_path(profile_dir: &Path) -> PathBuf {
    profile_dir
        .join(".ruuudy-launcher")
        .join("install-state.json")
}

fn profile_dir(manifest: &PackManifest) -> LauncherResult<PathBuf> {
    profile_dir_for_pack_id(&manifest.pack_id)
}

fn profile_dir_for_pack_id(pack_id: &str) -> LauncherResult<PathBuf> {
    Ok(launcher_data_dir()?.join("profiles").join(pack_id))
}

fn minecraft_dir() -> LauncherResult<PathBuf> {
    let app_data = std::env::var("APPDATA")
        .map_err(|_| LauncherError::Message("APPDATA is not set.".to_string()))?;
    Ok(PathBuf::from(app_data).join(".minecraft"))
}

fn minecraft_profile_id(manifest: &PackManifest) -> String {
    format!("ruuudy-{}", manifest.pack_id)
}

fn loader_version_id(
    loader_type: &str,
    loader_version: &str,
    minecraft_version: &str,
) -> LauncherResult<String> {
    let loader_type = normalize_loader_type(loader_type)?;
    let loader_version = loader_version.trim();
    let minecraft_version = minecraft_version.trim();
    match loader_type.as_str() {
        "vanilla" => Ok(minecraft_version.to_string()),
        "fabric" => Ok(format!(
            "fabric-loader-{loader_version}-{minecraft_version}"
        )),
        "forge" => Ok(format!("{minecraft_version}-forge-{loader_version}")),
        "neoforge" => Ok(format!("neoforge-{loader_version}")),
        _ => unreachable!("normalize_loader_type guards supported loaders"),
    }
}

fn forge_installer_url(minecraft_version: &str, loader_version: &str) -> String {
    let forge_version = format!("{}-{}", minecraft_version.trim(), loader_version.trim());
    format!(
        "https://maven.minecraftforge.net/net/minecraftforge/forge/{forge_version}/forge-{forge_version}-installer.jar"
    )
}

fn neoforge_installer_urls(minecraft_version: &str, loader_version: &str) -> Vec<String> {
    let minecraft_version = minecraft_version.trim();
    let loader_version = loader_version.trim();
    let modern = format!(
        "https://maven.neoforged.net/releases/net/neoforged/neoforge/{loader_version}/neoforge-{loader_version}-installer.jar"
    );
    let legacy_version = format!("{minecraft_version}-{loader_version}");
    let legacy = format!(
        "https://maven.neoforged.net/releases/net/neoforged/forge/{legacy_version}/forge-{legacy_version}-installer.jar"
    );
    vec![modern, legacy]
}

fn loader_display_name(loader: &Loader) -> &'static str {
    match loader.loader_type.as_str() {
        "vanilla" => "Vanilla",
        "fabric" => "Fabric",
        "forge" => "Forge",
        "neoforge" => "NeoForge",
        _ => "Modded",
    }
}

fn iso_timestamp() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn unix_timestamp() -> u64 {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    seconds
}

fn emit_progress(app: &AppHandle, stage: &str, message: &str, current: usize, total: usize) {
    let _ = app.emit(
        "install-progress",
        ProgressEvent {
            stage: stage.to_string(),
            message: message.to_string(),
            current,
            total,
        },
    );
}

fn read_zip_text<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    path: &str,
) -> LauncherResult<String> {
    let mut entry = archive
        .by_name(path)
        .map_err(|_| LauncherError::Message(format!("{path} not found in CurseForge zip.")))?;
    let mut text = String::new();
    entry.read_to_string(&mut text)?;
    Ok(text)
}

fn curseforge_loader(manifest: &CurseForgeManifest) -> LauncherResult<Loader> {
    let loader = manifest
        .minecraft
        .mod_loaders
        .iter()
        .find(|loader| loader.primary)
        .or_else(|| manifest.minecraft.mod_loaders.first())
        .ok_or_else(|| LauncherError::Message("CurseForge zip has no mod loader.".to_string()))?;
    let loader_id = loader.id.trim();
    for (loader_type, prefixes) in [
        ("fabric", &["fabric-loader-", "fabric-"][..]),
        ("forge", &["forge-"][..]),
        ("neoforge", &["neoforge-", "neo-forge-"][..]),
    ] {
        for prefix in prefixes {
            if let Some(version) = loader_id.strip_prefix(prefix) {
                if !version.trim().is_empty() {
                    return Ok(Loader {
                        loader_type: loader_type.to_string(),
                        version: version.to_string(),
                    });
                }
            }
        }
    }

    Err(LauncherError::Message(format!(
        "This launcher supports Vanilla, Fabric, Forge, and NeoForge packs, but this zip uses '{}' as its loader.",
        loader.id
    )))
}

fn slugify_pack_id(name: &str) -> String {
    let slug: String = name
        .trim()
        .to_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect();
    let slug = slug
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if slug.is_empty() {
        "imported-pack".to_string()
    } else {
        slug
    }
}

fn unique_pack_id(base: &str) -> LauncherResult<String> {
    let registry = read_registry()?;
    let existing: BTreeSet<String> = registry
        .profiles
        .into_iter()
        .map(|profile| profile.pack_id)
        .collect();
    let base = slugify_pack_id(base);
    if !existing.contains(&base) {
        return Ok(base);
    }

    for index in 2..1000 {
        let candidate = format!("{base}-{index}");
        if !existing.contains(&candidate) {
            return Ok(candidate);
        }
    }

    Err(LauncherError::Message(
        "Could not generate a unique local profile id.".to_string(),
    ))
}

fn share_code_from_pack_name(name: &str) -> String {
    let code: String = name
        .trim()
        .to_uppercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect();
    let code = code
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let code = if code.is_empty() {
        "IMPORTED-PACK".to_string()
    } else {
        code
    };
    code.chars().take(32).collect()
}

fn unique_share_code(base: &str) -> LauncherResult<String> {
    let registry = read_registry()?;
    let existing: BTreeSet<String> = registry
        .profiles
        .into_iter()
        .map(|profile| profile.code)
        .collect();
    let base = normalize_pack_code(base)?;
    if !existing.contains(&base) && base != "FAKERSBOB" {
        return Ok(base);
    }

    for index in 2..1000 {
        let suffix = format!("-{index}");
        let prefix_len = 32usize.saturating_sub(suffix.len());
        let candidate = format!(
            "{}{}",
            base.chars().take(prefix_len).collect::<String>(),
            suffix
        );
        if !existing.contains(&candidate) && candidate != "FAKERSBOB" {
            return Ok(candidate);
        }
    }

    Err(LauncherError::Message(
        "Could not generate a unique share code.".to_string(),
    ))
}

fn curseforge_download_to_manifest_file(
    name: &str,
    filename: &str,
    url: &str,
    sha256: String,
    size: u64,
) -> ManifestFile {
    ManifestFile::External {
        side: "client".to_string(),
        required: true,
        name: name.to_string(),
        filename: filename.to_string(),
        url: url.to_string(),
        sha256,
        size,
    }
}

fn download_curseforge_file(
    client: &Client,
    profile_dir: &Path,
    cf_file: &CurseForgeFile,
) -> LauncherResult<ImportedCurseForgeFile> {
    let url = format!(
        "https://www.curseforge.com/api/v1/mods/{}/files/{}/download",
        cf_file.project_id, cf_file.file_id
    );
    let mut response = client.get(url).send()?.error_for_status()?;
    let final_url = response.url().clone();
    let filename = final_url
        .path_segments()
        .and_then(|mut segments| segments.next_back())
        .and_then(|segment| urlencoding::decode(segment).ok())
        .map(|segment| segment.to_string())
        .filter(|segment| segment.ends_with(".jar"))
        .unwrap_or_else(|| format!("curseforge-{}-{}.jar", cf_file.project_id, cf_file.file_id));
    let relative_path = format!("mods/{filename}");
    let target = profile_dir.join(safe_relative_path(&relative_path)?);
    let mut bytes = Vec::new();
    response.read_to_end(&mut bytes)?;
    let sha256 = hex::encode(Sha256::digest(&bytes));
    let size = bytes.len() as u64;
    fs::write(target, bytes)?;
    let name = filename
        .strip_suffix(".jar")
        .unwrap_or(&filename)
        .replace(['-', '_'], " ");
    Ok(ImportedCurseForgeFile {
        manifest_file: curseforge_download_to_manifest_file(
            &name,
            &filename,
            final_url.as_str(),
            sha256,
            size,
        ),
        relative_path,
    })
}

fn extract_overrides<R: Read + std::io::Seek>(
    app: &AppHandle,
    archive: &mut ZipArchive<R>,
    profile_dir: &Path,
    overrides_root: &str,
    progress_offset: usize,
    total_steps: usize,
    managed_files: &mut Vec<String>,
) -> LauncherResult<Vec<ManifestOverride>> {
    let prefix = format!("{}/", overrides_root.trim_matches('/'));
    let mut overrides = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|error| {
            LauncherError::Message(format!("Could not read zip entry: {error}"))
        })?;
        let name = entry.name().replace('\\', "/");
        if !name.starts_with(&prefix) || entry.is_dir() {
            continue;
        }
        let relative = name
            .strip_prefix(&prefix)
            .ok_or_else(|| LauncherError::Message("Invalid override path.".to_string()))?
            .to_string();
        let safe = safe_relative_path(&relative)?;
        emit_progress(
            app,
            "overrides",
            &format!("Extracting {}", relative),
            progress_offset + overrides.len() + 1,
            total_steps,
        );
        let target = profile_dir.join(safe);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        fs::write(&target, &bytes)?;
        let sha256 = hex::encode(Sha256::digest(&bytes));
        let size = bytes.len() as u64;
        managed_files.push(relative.clone());
        if is_managed_override_file(&relative) {
            overrides.push(ManifestOverride {
                path: relative.clone(),
                url: format!("{LOCAL_PENDING_URL_PREFIX}{relative}"),
                sha256,
                size,
            });
        }
    }
    Ok(overrides)
}

fn is_resource_pack_zip(relative_path: &str) -> bool {
    let normalized = relative_path.replace('\\', "/").to_lowercase();
    normalized.starts_with("resourcepacks/") && normalized.ends_with(".zip")
}

fn is_shader_pack_zip(relative_path: &str) -> bool {
    let normalized = relative_path.replace('\\', "/").to_lowercase();
    normalized.starts_with("shaderpacks/") && normalized.ends_with(".zip")
}

fn resource_pack_options_entry(relative_path: &str) -> Option<String> {
    if !is_resource_pack_zip(relative_path) {
        return None;
    }

    let normalized = relative_path.replace('\\', "/");
    normalized
        .rsplit('/')
        .next()
        .filter(|filename| !filename.is_empty())
        .map(|filename| format!("file/{filename}"))
}

fn manifest_resource_pack_entries(manifest: &PackManifest) -> Vec<String> {
    let mut entries = Vec::new();
    for override_file in &manifest.overrides {
        if let Some(entry) = resource_pack_options_entry(&override_file.path) {
            if !entries.contains(&entry) {
                entries.push(entry);
            }
        }
    }
    entries
}

fn shader_pack_filename(relative_path: &str) -> Option<String> {
    if !is_shader_pack_zip(relative_path) {
        return None;
    }

    let normalized = relative_path.replace('\\', "/");
    normalized
        .rsplit('/')
        .next()
        .filter(|filename| !filename.is_empty())
        .map(|filename| filename.to_string())
}

fn manifest_shader_pack_filenames(manifest: &PackManifest) -> Vec<String> {
    let mut entries = Vec::new();
    for override_file in &manifest.overrides {
        if let Some(entry) = shader_pack_filename(&override_file.path) {
            if !entries.contains(&entry) {
                entries.push(entry);
            }
        }
    }
    entries
}

fn parse_options_list(value: &str) -> Vec<String> {
    serde_json::from_str(value.trim()).unwrap_or_default()
}

fn read_options_list(lines: &[String], key: &str) -> Vec<String> {
    let prefix = format!("{key}:");
    lines
        .iter()
        .find_map(|line| line.strip_prefix(&prefix))
        .map(parse_options_list)
        .unwrap_or_default()
}

fn upsert_options_list(
    lines: &mut Vec<String>,
    key: &str,
    values: &[String],
) -> LauncherResult<()> {
    let next_line = format!("{key}:{}", serde_json::to_string(values)?);
    let prefix = format!("{key}:");
    if let Some(line) = lines.iter_mut().find(|line| line.starts_with(&prefix)) {
        *line = next_line;
    } else {
        lines.push(next_line);
    }
    Ok(())
}

fn sync_resource_pack_options(
    profile_dir: &Path,
    manifest: &PackManifest,
    previous_state: Option<&LocalInstallState>,
) -> LauncherResult<()> {
    let current_entries = manifest_resource_pack_entries(manifest);
    if current_entries.is_empty() && previous_state.is_none() {
        return Ok(());
    }

    let current_set: BTreeSet<String> = current_entries.iter().cloned().collect();
    let stale_entries: BTreeSet<String> = previous_state
        .map(|state| {
            state
                .managed_files
                .iter()
                .filter_map(|path| resource_pack_options_entry(path))
                .filter(|entry| !current_set.contains(entry))
                .collect()
        })
        .unwrap_or_default();

    if current_entries.is_empty() && stale_entries.is_empty() {
        return Ok(());
    }

    let options_path = profile_dir.join("options.txt");
    let mut lines = if options_path.exists() {
        fs::read_to_string(&options_path)?
            .lines()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    let mut resource_packs = read_options_list(&lines, "resourcePacks")
        .into_iter()
        .filter(|entry| !stale_entries.contains(entry))
        .collect::<Vec<_>>();
    if !resource_packs.iter().any(|entry| entry == "vanilla") {
        resource_packs.insert(0, "vanilla".to_string());
    }
    for entry in current_entries {
        if !resource_packs.contains(&entry) {
            resource_packs.push(entry);
        }
    }

    let mut incompatible_packs = read_options_list(&lines, "incompatibleResourcePacks")
        .into_iter()
        .filter(|entry| !current_set.contains(entry) && !stale_entries.contains(entry))
        .collect::<Vec<_>>();
    incompatible_packs.dedup();

    upsert_options_list(&mut lines, "resourcePacks", &resource_packs)?;
    upsert_options_list(&mut lines, "incompatibleResourcePacks", &incompatible_packs)?;

    if let Some(parent) = options_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(options_path, format!("{}\n", lines.join("\n")))?;
    Ok(())
}

fn read_properties(lines: &[String], key: &str) -> Option<String> {
    let prefix = format!("{key}=");
    lines
        .iter()
        .find_map(|line| line.strip_prefix(&prefix))
        .map(|value| value.to_string())
}

fn upsert_property(lines: &mut Vec<String>, key: &str, value: &str) {
    let next_line = format!("{key}={value}");
    let prefix = format!("{key}=");
    if let Some(line) = lines.iter_mut().find(|line| line.starts_with(&prefix)) {
        *line = next_line;
    } else {
        lines.push(next_line);
    }
}

fn remove_property(lines: &mut Vec<String>, key: &str) {
    let prefix = format!("{key}=");
    lines.retain(|line| !line.starts_with(&prefix));
}

fn sync_shader_pack_options(
    profile_dir: &Path,
    manifest: &PackManifest,
    previous_state: Option<&LocalInstallState>,
) -> LauncherResult<()> {
    let current_packs = manifest_shader_pack_filenames(manifest);
    if current_packs.is_empty() && previous_state.is_none() {
        return Ok(());
    }

    let current_set: BTreeSet<String> = current_packs.iter().cloned().collect();
    let stale_packs: BTreeSet<String> = previous_state
        .map(|state| {
            state
                .managed_files
                .iter()
                .filter_map(|path| shader_pack_filename(path))
                .filter(|entry| !current_set.contains(entry))
                .collect()
        })
        .unwrap_or_default();

    if current_packs.is_empty() && stale_packs.is_empty() {
        return Ok(());
    }

    let config_path = profile_dir.join("config").join("iris.properties");
    let mut lines = if config_path.exists() {
        fs::read_to_string(&config_path)?
            .lines()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    let current_shader = read_properties(&lines, "shaderPack");
    let next_shader = if current_shader
        .as_ref()
        .is_some_and(|shader| current_set.contains(shader))
    {
        current_shader.clone()
    } else {
        current_packs.last().cloned()
    };

    if let Some(shader_pack) = next_shader {
        upsert_property(&mut lines, "enableShaders", "true");
        upsert_property(&mut lines, "shaderPack", &shader_pack);
    } else if current_shader
        .as_ref()
        .is_some_and(|shader| stale_packs.contains(shader))
    {
        upsert_property(&mut lines, "enableShaders", "false");
        remove_property(&mut lines, "shaderPack");
    } else {
        return Ok(());
    }

    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(config_path, format!("{}\n", lines.join("\n")))?;
    Ok(())
}

fn is_managed_override_file(relative_path: &str) -> bool {
    let normalized = relative_path.replace('\\', "/");
    if safe_relative_path(&normalized).is_err() {
        return false;
    }
    let lower = normalized.to_lowercase();
    if lower.is_empty()
        || lower == "manifest.json"
        || lower == "modlist.html"
        || lower.ends_with('/')
        || lower.starts_with(".ruuudy-launcher/")
        || lower.starts_with("saves/")
        || lower.starts_with("crash-reports/")
        || lower.starts_with("logs/")
    {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugifies_imported_pack_names() {
        assert_eq!(
            slugify_pack_id("Fakersbob Cobblemon++"),
            "fakersbob-cobblemon"
        );
        assert_eq!(slugify_pack_id("   "), "imported-pack");
    }

    #[test]
    fn extracts_primary_fabric_loader_version_from_curseforge_manifest() {
        let manifest = CurseForgeManifest {
            name: "Fakersbob".to_string(),
            overrides: Some("overrides".to_string()),
            files: Vec::new(),
            minecraft: CurseForgeMinecraft {
                version: "1.21.1".to_string(),
                mod_loaders: vec![
                    CurseForgeModLoader {
                        id: "forge-52.0.0".to_string(),
                        primary: false,
                    },
                    CurseForgeModLoader {
                        id: "fabric-0.19.2".to_string(),
                        primary: true,
                    },
                ],
            },
        };

        let loader = curseforge_loader(&manifest).unwrap();
        assert_eq!(loader.loader_type, "fabric");
        assert_eq!(loader.version, "0.19.2");
    }

    #[test]
    fn accepts_fabric_loader_prefix_from_curseforge_manifest() {
        let manifest = CurseForgeManifest {
            name: "Future Fabric Pack".to_string(),
            overrides: Some("overrides".to_string()),
            files: Vec::new(),
            minecraft: CurseForgeMinecraft {
                version: "1.22".to_string(),
                mod_loaders: vec![CurseForgeModLoader {
                    id: "fabric-loader-0.20.0".to_string(),
                    primary: true,
                }],
            },
        };

        let loader = curseforge_loader(&manifest).unwrap();
        assert_eq!(loader.loader_type, "fabric");
        assert_eq!(loader.version, "0.20.0");
    }

    #[test]
    fn extracts_forge_loader_from_curseforge_manifest() {
        let manifest = CurseForgeManifest {
            name: "Forge Pack".to_string(),
            overrides: None,
            files: Vec::new(),
            minecraft: CurseForgeMinecraft {
                version: "1.21.1".to_string(),
                mod_loaders: vec![CurseForgeModLoader {
                    id: "forge-52.0.0".to_string(),
                    primary: true,
                }],
            },
        };

        let loader = curseforge_loader(&manifest).unwrap();

        assert_eq!(loader.loader_type, "forge");
        assert_eq!(loader.version, "52.0.0");
    }

    #[test]
    fn extracts_neoforge_loader_from_curseforge_manifest() {
        let manifest = CurseForgeManifest {
            name: "NeoForge Pack".to_string(),
            overrides: None,
            files: Vec::new(),
            minecraft: CurseForgeMinecraft {
                version: "1.21.1".to_string(),
                mod_loaders: vec![CurseForgeModLoader {
                    id: "neoforge-21.1.200".to_string(),
                    primary: true,
                }],
            },
        };

        let loader = curseforge_loader(&manifest).unwrap();

        assert_eq!(loader.loader_type, "neoforge");
        assert_eq!(loader.version, "21.1.200");
    }

    #[test]
    fn creates_loader_specific_launcher_version_ids() {
        assert_eq!(
            loader_version_id("fabric", "0.19.2", "1.21.1").unwrap(),
            "fabric-loader-0.19.2-1.21.1"
        );
        assert_eq!(
            loader_version_id("forge", "52.0.0", "1.21.1").unwrap(),
            "1.21.1-forge-52.0.0"
        );
        assert_eq!(
            loader_version_id("neoforge", "21.1.200", "1.21.1").unwrap(),
            "neoforge-21.1.200"
        );
        assert_eq!(
            loader_version_id("vanilla", "", "1.21.1").unwrap(),
            "1.21.1"
        );
    }

    #[test]
    fn creates_forge_installer_url() {
        assert_eq!(
            forge_installer_url("1.20.1", "47.4.4"),
            "https://maven.minecraftforge.net/net/minecraftforge/forge/1.20.1-47.4.4/forge-1.20.1-47.4.4-installer.jar"
        );
    }

    #[test]
    fn creates_neoforge_installer_url_candidates() {
        assert_eq!(
            neoforge_installer_urls("1.21.1", "21.1.200"),
            vec![
                "https://maven.neoforged.net/releases/net/neoforged/neoforge/21.1.200/neoforge-21.1.200-installer.jar".to_string(),
                "https://maven.neoforged.net/releases/net/neoforged/forge/1.21.1-21.1.200/forge-1.21.1-21.1.200-installer.jar".to_string()
            ]
        );
    }

    #[test]
    fn rejects_unsafe_relative_paths() {
        assert!(safe_relative_path("mods/fabric-api.jar").is_ok());
        assert!(safe_relative_path("../secret.txt").is_err());
        assert!(safe_relative_path("mods/../../secret.txt").is_err());
        assert!(safe_relative_path("C:/Windows/win.ini").is_err());
    }

    #[test]
    fn launcher_timestamps_are_iso_8601() {
        let timestamp = iso_timestamp();

        assert!(timestamp.contains('T'));
        assert!(timestamp.ends_with('Z'));
    }

    #[test]
    fn creates_share_codes_from_pack_names() {
        assert_eq!(
            share_code_from_pack_name("Fakersbob Cobblemon++"),
            "FAKERSBOB-COBBLEMON"
        );
        assert_eq!(share_code_from_pack_name("   "), "IMPORTED-PACK");
    }

    #[test]
    fn curseforge_downloads_become_locked_external_manifest_files() {
        let file = curseforge_download_to_manifest_file(
            "Cobblemon Additions",
            "cobblemon-additions.jar",
            "https://mediafilez.forgecdn.net/files/example/cobblemon-additions.jar",
            "1".repeat(64),
            42,
        );

        match file {
            ManifestFile::External {
                name,
                filename,
                url,
                sha256,
                size,
                ..
            } => {
                assert_eq!(name, "Cobblemon Additions");
                assert_eq!(filename, "cobblemon-additions.jar");
                assert_eq!(
                    url,
                    "https://mediafilez.forgecdn.net/files/example/cobblemon-additions.jar"
                );
                assert_eq!(sha256, "1".repeat(64));
                assert_eq!(size, 42);
            }
            ManifestFile::Modrinth { .. } => {
                panic!("CurseForge imports should lock as external files")
            }
        }
    }

    #[test]
    fn install_plan_applies_default_options_only_on_first_install() {
        let manifest = PackManifest {
            schema_version: 1,
            pack_id: "fakersbob".to_string(),
            pack_name: "Fakersbob".to_string(),
            version: "manual-1".to_string(),
            minecraft_version: "1.21.1".to_string(),
            loader: Loader {
                loader_type: "fabric".to_string(),
                version: "0.19.2".to_string(),
            },
            server: Server {
                address: "mc.ruuudy.in".to_string(),
                port: 25565,
            },
            files: Vec::new(),
            overrides: Vec::new(),
            default_options: Some(ManifestOverride {
                path: "options.txt".to_string(),
                url: "https://launcher.ruuudy.in/api/packs/JUNFEET/files/defaults/options.txt"
                    .to_string(),
                sha256: "a".repeat(64),
                size: 42,
            }),
        };

        let first_install = build_install_plan(&manifest, None);
        let resync = build_install_plan(
            &manifest,
            Some(&LocalInstallState {
                pack_id: "fakersbob".to_string(),
                manifest_version: "manual-1".to_string(),
                managed_files: Vec::new(),
            }),
        );

        assert_eq!(first_install.downloads.len(), 1);
        assert_eq!(first_install.downloads[0].relative_path, "options.txt");
        assert!(first_install.next_managed_files.is_empty());
        assert!(resync.downloads.is_empty());
        assert!(resync.next_managed_files.is_empty());
    }

    #[test]
    fn local_jar_import_accepts_multiple_jars_and_replaces_by_filename() {
        let root = std::env::temp_dir().join(format!(
            "ruuudy-launcher-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let source_dir = root.join("source");
        let profile_dir = root.join("profile");
        fs::create_dir_all(&source_dir).unwrap();
        fs::create_dir_all(profile_dir.join("mods")).unwrap();
        let first = source_dir.join("alpha.jar");
        let second = source_dir.join("beta.jar");
        fs::write(&first, b"alpha-new").unwrap();
        fs::write(&second, b"beta").unwrap();

        let manifest = PackManifest {
            schema_version: 1,
            pack_id: "fakersbob".to_string(),
            pack_name: "Fakersbob".to_string(),
            version: "manual-old".to_string(),
            minecraft_version: "1.21.1".to_string(),
            loader: Loader {
                loader_type: "fabric".to_string(),
                version: "0.19.2".to_string(),
            },
            server: Server {
                address: "mc.ruuudy.in".to_string(),
                port: 25565,
            },
            files: Vec::new(),
            overrides: vec![ManifestOverride {
                path: "mods/alpha.jar".to_string(),
                url: "local-pending://mods/alpha.jar".to_string(),
                sha256: "0".repeat(64),
                size: 1,
            }],
            default_options: None,
        };

        let next_manifest =
            add_local_jar_overrides(manifest, &profile_dir, &[first.clone(), second.clone()])
                .unwrap();

        let override_paths = next_manifest
            .overrides
            .iter()
            .map(|override_file| override_file.path.as_str())
            .collect::<Vec<_>>();
        assert_eq!(override_paths, vec!["mods/alpha.jar", "mods/beta.jar"]);
        assert_eq!(
            fs::read(profile_dir.join("mods/alpha.jar")).unwrap(),
            b"alpha-new"
        );
        assert_eq!(
            fs::read(profile_dir.join("mods/beta.jar")).unwrap(),
            b"beta"
        );
        assert!(next_manifest.version.starts_with("manual-"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn local_resource_pack_import_adds_resourcepack_overrides() {
        let root = std::env::temp_dir().join(format!(
            "ruuudy-launcher-resourcepack-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let source_dir = root.join("source");
        let profile_dir = root.join("profile");
        fs::create_dir_all(&source_dir).unwrap();
        let pack = source_dir.join("RCT-Trainer-Textures-Plus.zip");
        fs::write(&pack, b"resource-pack").unwrap();

        let manifest = PackManifest {
            schema_version: 1,
            pack_id: "fakersbob".to_string(),
            pack_name: "Fakersbob".to_string(),
            version: "manual-old".to_string(),
            minecraft_version: "1.21.1".to_string(),
            loader: Loader {
                loader_type: "fabric".to_string(),
                version: "0.19.2".to_string(),
            },
            server: Server {
                address: "mc.ruuudy.in".to_string(),
                port: 25565,
            },
            files: Vec::new(),
            overrides: Vec::new(),
            default_options: None,
        };

        let next_manifest =
            add_local_resource_pack_overrides(manifest, &profile_dir, &[pack.clone()]).unwrap();

        assert_eq!(next_manifest.overrides.len(), 1);
        assert_eq!(
            next_manifest.overrides[0].path,
            "resourcepacks/RCT-Trainer-Textures-Plus.zip"
        );
        assert_eq!(
            fs::read(profile_dir.join("resourcepacks/RCT-Trainer-Textures-Plus.zip")).unwrap(),
            b"resource-pack"
        );
        assert!(is_managed_override_file(&next_manifest.overrides[0].path));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn managed_override_files_include_mod_jars_and_resource_pack_zips() {
        assert!(is_managed_override_file("mods/local-mod.jar"));
        assert!(is_managed_override_file("resourcepacks/RCT.zip"));
        assert!(is_managed_override_file("shaderpacks/Complementary.zip"));
        assert!(is_managed_override_file(
            "config/defaultoptions/options.txt"
        ));
        assert!(is_managed_override_file("defaultconfigs/forge-server.toml"));
        assert!(is_managed_override_file(
            "kubejs/startup_scripts/biohazard.js"
        ));
        assert!(is_managed_override_file("tacz/custom/gun.json"));
        assert!(!is_managed_override_file("saves/Test World/level.dat"));
        assert!(!is_managed_override_file("logs/latest.log"));
        assert!(!is_managed_override_file("../outside.txt"));
    }

    #[test]
    fn official_launcher_profile_is_selected_after_upsert() {
        let root = std::env::temp_dir().join(format!(
            "ruuudy-launcher-selected-profile-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let previous_appdata = std::env::var("APPDATA").ok();
        std::env::set_var("APPDATA", &root);

        let manifest = PackManifest {
            schema_version: 1,
            pack_id: "biohazard-public-beta-bentoboob".to_string(),
            pack_name: "Biohazard Public Beta".to_string(),
            version: "manual-new".to_string(),
            minecraft_version: "1.20.1".to_string(),
            loader: Loader {
                loader_type: "forge".to_string(),
                version: "47.4.0".to_string(),
            },
            server: Server {
                address: "mc.ruuudy.in".to_string(),
                port: 25565,
            },
            files: Vec::new(),
            overrides: Vec::new(),
            default_options: None,
        };
        let profile_dir = profile_dir(&manifest).unwrap();

        upsert_official_launcher_profile(&manifest, &profile_dir, Some("BENTOBOOB")).unwrap();

        let launcher_profiles =
            fs::read_to_string(root.join(".minecraft/launcher_profiles.json")).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&launcher_profiles).unwrap();
        let profile_id = minecraft_profile_id(&manifest);
        assert_eq!(
            parsed
                .get("selectedProfile")
                .and_then(|value| value.as_str()),
            Some(profile_id.as_str())
        );
        assert_eq!(
            parsed["profiles"][profile_id.as_str()]["name"].as_str(),
            Some("BENTOBOOB Server")
        );
        assert_eq!(
            parsed["profiles"][profile_id.as_str()]["lastVersionId"].as_str(),
            Some("1.20.1-forge-47.4.0")
        );

        if let Some(appdata) = previous_appdata {
            std::env::set_var("APPDATA", appdata);
        } else {
            std::env::remove_var("APPDATA");
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn local_shader_pack_import_adds_shaderpack_overrides() {
        let root = std::env::temp_dir().join(format!(
            "ruuudy-launcher-shaderpack-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let source_dir = root.join("source");
        let profile_dir = root.join("profile");
        fs::create_dir_all(&source_dir).unwrap();
        let pack = source_dir.join("Complementary.zip");
        fs::write(&pack, b"shader-pack").unwrap();

        let manifest = PackManifest {
            schema_version: 1,
            pack_id: "fakersbob".to_string(),
            pack_name: "Fakersbob".to_string(),
            version: "manual-old".to_string(),
            minecraft_version: "1.21.1".to_string(),
            loader: Loader {
                loader_type: "fabric".to_string(),
                version: "0.19.2".to_string(),
            },
            server: Server {
                address: "mc.ruuudy.in".to_string(),
                port: 25565,
            },
            files: Vec::new(),
            overrides: Vec::new(),
            default_options: None,
        };

        let next_manifest =
            add_local_shader_pack_overrides(manifest, &profile_dir, &[pack.clone()]).unwrap();

        assert_eq!(next_manifest.overrides.len(), 1);
        assert_eq!(
            next_manifest.overrides[0].path,
            "shaderpacks/Complementary.zip"
        );
        assert_eq!(
            fs::read(profile_dir.join("shaderpacks/Complementary.zip")).unwrap(),
            b"shader-pack"
        );
        assert!(is_managed_override_file(&next_manifest.overrides[0].path));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn sync_shader_packs_enables_managed_shader_in_iris_config() {
        let root = std::env::temp_dir().join(format!(
            "ruuudy-launcher-shaderpack-options-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let profile_dir = root.join("profile");
        fs::create_dir_all(profile_dir.join("config")).unwrap();
        fs::write(
            profile_dir.join("config").join("iris.properties"),
            "maxShadowRenderDistance=32\nshaderPack=Personal.zip\n",
        )
        .unwrap();

        let manifest = PackManifest {
            schema_version: 1,
            pack_id: "fakersbob".to_string(),
            pack_name: "Fakersbob".to_string(),
            version: "manual-new".to_string(),
            minecraft_version: "1.21.1".to_string(),
            loader: Loader {
                loader_type: "fabric".to_string(),
                version: "0.19.2".to_string(),
            },
            server: Server {
                address: "mc.ruuudy.in".to_string(),
                port: 25565,
            },
            files: Vec::new(),
            overrides: vec![ManifestOverride {
                path: "shaderpacks/Complementary.zip".to_string(),
                url: "local-pending://shaderpacks/Complementary.zip".to_string(),
                sha256: "3".repeat(64),
                size: 10,
            }],
            default_options: None,
        };

        sync_shader_pack_options(&profile_dir, &manifest, None).unwrap();

        let config =
            fs::read_to_string(profile_dir.join("config").join("iris.properties")).unwrap();
        assert!(config.contains("maxShadowRenderDistance=32"));
        assert!(config.contains("enableShaders=true"));
        assert!(config.contains("shaderPack=Complementary.zip"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn sync_resource_packs_enables_managed_packs_in_options() {
        let root = std::env::temp_dir().join(format!(
            "ruuudy-launcher-resourcepack-options-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let profile_dir = root.join("profile");
        fs::create_dir_all(&profile_dir).unwrap();
        fs::write(
            profile_dir.join("options.txt"),
            "resourcePacks:[\"vanilla\",\"file/Personal.zip\"]\nincompatibleResourcePacks:[\"file/RCT.zip\"]\n",
        )
        .unwrap();

        let manifest = PackManifest {
            schema_version: 1,
            pack_id: "fakersbob".to_string(),
            pack_name: "Fakersbob".to_string(),
            version: "manual-new".to_string(),
            minecraft_version: "1.21.1".to_string(),
            loader: Loader {
                loader_type: "fabric".to_string(),
                version: "0.19.2".to_string(),
            },
            server: Server {
                address: "mc.ruuudy.in".to_string(),
                port: 25565,
            },
            files: Vec::new(),
            overrides: vec![ManifestOverride {
                path: "resourcepacks/RCT.zip".to_string(),
                url: "local-pending://resourcepacks/RCT.zip".to_string(),
                sha256: "1".repeat(64),
                size: 10,
            }],
            default_options: None,
        };

        sync_resource_pack_options(&profile_dir, &manifest, None).unwrap();

        let options = fs::read_to_string(profile_dir.join("options.txt")).unwrap();
        assert!(
            options.contains("resourcePacks:[\"vanilla\",\"file/Personal.zip\",\"file/RCT.zip\"]")
        );
        assert!(options.contains("incompatibleResourcePacks:[]"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn sync_resource_packs_removes_stale_managed_packs_from_options() {
        let root = std::env::temp_dir().join(format!(
            "ruuudy-launcher-resourcepack-stale-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let profile_dir = root.join("profile");
        fs::create_dir_all(&profile_dir).unwrap();
        fs::write(
            profile_dir.join("options.txt"),
            "resourcePacks:[\"vanilla\",\"file/Old.zip\",\"file/Personal.zip\"]\n",
        )
        .unwrap();

        let manifest = PackManifest {
            schema_version: 1,
            pack_id: "fakersbob".to_string(),
            pack_name: "Fakersbob".to_string(),
            version: "manual-new".to_string(),
            minecraft_version: "1.21.1".to_string(),
            loader: Loader {
                loader_type: "fabric".to_string(),
                version: "0.19.2".to_string(),
            },
            server: Server {
                address: "mc.ruuudy.in".to_string(),
                port: 25565,
            },
            files: Vec::new(),
            overrides: vec![ManifestOverride {
                path: "resourcepacks/New.zip".to_string(),
                url: "local-pending://resourcepacks/New.zip".to_string(),
                sha256: "2".repeat(64),
                size: 10,
            }],
            default_options: None,
        };
        let previous_state = LocalInstallState {
            pack_id: "fakersbob".to_string(),
            manifest_version: "manual-old".to_string(),
            managed_files: vec!["resourcepacks/Old.zip".to_string()],
        };

        sync_resource_pack_options(&profile_dir, &manifest, Some(&previous_state)).unwrap();

        let options = fs::read_to_string(profile_dir.join("options.txt")).unwrap();
        assert!(
            options.contains("resourcePacks:[\"vanilla\",\"file/Personal.zip\",\"file/New.zip\"]")
        );
        assert!(!options.contains("file/Old.zip"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn saving_published_manifest_does_not_mark_profile_installed() {
        let root = std::env::temp_dir().join(format!(
            "ruuudy-launcher-publish-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let previous_appdata = std::env::var("APPDATA").ok();
        std::env::set_var("APPDATA", &root);

        let manifest = PackManifest {
            schema_version: 1,
            pack_id: "fakersbob".to_string(),
            pack_name: "Fakersbob".to_string(),
            version: "manual-new".to_string(),
            minecraft_version: "1.21.1".to_string(),
            loader: Loader {
                loader_type: "fabric".to_string(),
                version: "0.19.2".to_string(),
            },
            server: Server {
                address: "mc.ruuudy.in".to_string(),
                port: 25565,
            },
            files: Vec::new(),
            overrides: Vec::new(),
            default_options: None,
        };
        let profile_dir = profile_dir(&manifest).unwrap();
        fs::create_dir_all(profile_dir.join(".ruuudy-launcher")).unwrap();
        write_install_state(
            &profile_dir,
            &LocalInstallState {
                pack_id: manifest.pack_id.clone(),
                manifest_version: "manual-old".to_string(),
                managed_files: Vec::new(),
            },
        )
        .unwrap();

        save_published_manifest("JUNFEET", &manifest).unwrap();

        let status = get_install_status(manifest).unwrap();
        assert!(!status.installed);
        assert_eq!(status.installed_version.as_deref(), Some("manual-old"));
        assert_eq!(status.latest_version, "manual-new");

        if let Some(appdata) = previous_appdata {
            std::env::set_var("APPDATA", appdata);
        } else {
            std::env::remove_var("APPDATA");
        }
        let _ = fs::remove_dir_all(root);
    }
}
