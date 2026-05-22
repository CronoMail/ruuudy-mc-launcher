use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256, Sha512};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter};
use thiserror::Error;
use zip::ZipArchive;

const FAKERSBOB_MANIFEST: &str = include_str!("../packs/fakersbob/manifest.json");
const LOCAL_PENDING_URL_PREFIX: &str = "local-pending://";

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
    loader_version: String,
    curseforge_mods: usize,
    overrides: usize,
    minecraft_profile_id: String,
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

    emit_progress(
        &app,
        "fabric",
        "Installing Fabric launcher profile",
        total_steps - 1,
        total_steps,
    );
    install_fabric_profile(&client, &manifest)?;
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
    let loader_version = curseforge_loader_version(&curseforge)?;
    let pack_id = slugify_pack_id(&curseforge.name);
    let code = unique_share_code(&share_code_from_pack_name(&curseforge.name))?;
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
        loader: Loader {
            loader_type: "fabric".to_string(),
            version: loader_version,
        },
        server: Server {
            address: "mc.ruuudy.in".to_string(),
            port: 25565,
        },
        files: locked_files,
        overrides: override_files,
    };

    emit_progress(
        &app,
        "fabric",
        "Installing Fabric launcher profile",
        total_steps - 1,
        total_steps,
    );
    install_fabric_profile(&client, &manifest)?;
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
fn sync_manifest_with_profile_folder(
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
fn publish_profile(
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
    let (publish_manifest, uploaded_files) =
        upload_unrepresented_managed_files(&client, &api_base, admin_token.trim(), &code, manifest)?;
    client
        .put(format!("{api_base}/api/admin/packs/{code}"))
        .bearer_auth(admin_token.trim())
        .json(&publish_manifest)
        .send()?
        .error_for_status()?;
    save_local_manifest(&code, &publish_manifest)?;

    Ok(PublishSummary {
        code: code.clone(),
        manifest_url: format!("{api_base}/api/packs/{code}"),
        uploaded_files,
        manifest: publish_manifest,
    })
}

#[tauri::command]
fn search_modrinth_mods(
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
fn add_modrinth_mod_to_profile(
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
fn import_local_jar_to_profile(
    code: String,
    manifest: PackManifest,
    jar_path: String,
) -> LauncherResult<PackManifest> {
    let code = normalize_pack_code(&code)?;
    validate_manifest(&manifest)?;
    let source_path = PathBuf::from(jar_path);
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
    let profile_dir = profile_dir(&manifest)?;
    let target = profile_dir.join(safe_relative_path(&relative_path)?);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }

    let bytes = fs::read(&source_path)?;
    let sha256 = hex::encode(Sha256::digest(&bytes));
    let size = bytes.len() as u64;
    fs::write(&target, bytes)?;

    let mut next_manifest = manifest;
    next_manifest
        .files
        .retain(|file| manifest_file_relative_path(file) != relative_path);
    next_manifest
        .overrides
        .retain(|override_file| override_file.path.replace('\\', "/") != relative_path);
    next_manifest.overrides.push(ManifestOverride {
        path: relative_path.clone(),
        url: format!("{LOCAL_PENDING_URL_PREFIX}{relative_path}"),
        sha256,
        size,
    });
    next_manifest
        .overrides
        .sort_by(|left, right| left.path.cmp(&right.path));
    next_manifest.version = format!("manual-{}", unix_timestamp());

    let next_state = LocalInstallState {
        pack_id: next_manifest.pack_id.clone(),
        manifest_version: next_manifest.version.clone(),
        managed_files: build_install_plan(&next_manifest, None).next_managed_files,
    };
    write_install_state(&profile_dir, &next_state)?;
    save_profile_manifest(code, next_manifest.clone())?;
    Ok(next_manifest)
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
            install_pack,
            install_profile_pack,
            import_curseforge_zip,
            delete_profile,
            export_profile_manifest,
            save_profile_manifest,
            sync_manifest_with_profile_folder,
            publish_profile,
            search_modrinth_mods,
            add_modrinth_mod_to_profile,
            import_local_jar_to_profile,
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

    if manifest.loader.loader_type != "fabric" {
        return Err(LauncherError::Message(
            "Only Fabric packs are supported in this launcher version.".to_string(),
        ));
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

    Ok(())
}

fn is_hex_hash(value: &str, expected_len: usize) -> bool {
    value.len() == expected_len && value.chars().all(|ch| ch.is_ascii_hexdigit())
}

fn build_install_plan(manifest: &PackManifest, state: Option<&LocalInstallState>) -> InstallPlan {
    let downloads: Vec<DownloadItem> = manifest
        .files
        .iter()
        .map(file_to_download)
        .chain(manifest.overrides.iter().map(override_to_download))
        .collect();
    let next_managed_files: Vec<String> = downloads
        .iter()
        .map(|item| item.relative_path.clone())
        .collect();
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
        .retain(|override_file| is_override_mod_jar(&override_file.path));
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

    let mut represented: BTreeSet<String> =
        publish_manifest.files.iter().map(manifest_file_relative_path).collect();
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
        if !is_override_mod_jar(&relative_path) {
            continue;
        }

        if !profile_dir.join(safe_relative_path(&relative_path)?).is_file() {
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
        let next_state = LocalInstallState {
            pack_id: publish_manifest.pack_id.clone(),
            manifest_version: publish_manifest.version.clone(),
            managed_files: build_install_plan(&publish_manifest, None).next_managed_files,
        };
        write_install_state(&profile_dir, &next_state)?;
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

fn install_fabric_profile(client: &Client, manifest: &PackManifest) -> LauncherResult<()> {
    let minecraft_dir = minecraft_dir()?;
    let version_id = fabric_version_id(&manifest.loader.version, &manifest.minecraft_version);
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
        "lastVersionId": fabric_version_id(&manifest.loader.version, &manifest.minecraft_version)
    });

    if !root
        .get("profiles")
        .is_some_and(|profiles| profiles.is_object())
    {
        root["profiles"] = serde_json::json!({});
    }
    root["profiles"][profile_id] = profile;

    if let Some(parent) = launcher_profiles.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(launcher_profiles, serde_json::to_string_pretty(&root)?)?;
    Ok(())
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
        .retain(|existing| existing.code != profile.code && existing.pack_id != profile.pack_id);
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

fn fabric_version_id(loader_version: &str, minecraft_version: &str) -> String {
    format!("fabric-loader-{loader_version}-{minecraft_version}")
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

fn curseforge_loader_version(manifest: &CurseForgeManifest) -> LauncherResult<String> {
    let loader = manifest
        .minecraft
        .mod_loaders
        .iter()
        .find(|loader| loader.primary)
        .or_else(|| manifest.minecraft.mod_loaders.first())
        .ok_or_else(|| LauncherError::Message("CurseForge zip has no mod loader.".to_string()))?;
    loader
        .id
        .strip_prefix("fabric-")
        .map(|version| version.to_string())
        .ok_or_else(|| {
            LauncherError::Message("Only Fabric CurseForge zips are supported.".to_string())
        })
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
        if is_override_mod_jar(&relative) {
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

fn is_override_mod_jar(relative_path: &str) -> bool {
    let normalized = relative_path.replace('\\', "/").to_lowercase();
    normalized.starts_with("mods/") && normalized.ends_with(".jar")
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

        assert_eq!(curseforge_loader_version(&manifest).unwrap(), "0.19.2");
    }

    #[test]
    fn rejects_non_fabric_curseforge_exports() {
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

        assert!(curseforge_loader_version(&manifest).is_err());
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
}
