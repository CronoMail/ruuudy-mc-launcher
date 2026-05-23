import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";
import {
  CheckCircle2,
  Copy,
  Download,
  FolderOpen,
  Gamepad2,
  PackageOpen,
  Plus,
  RefreshCcw,
  Search,
  Send,
  Settings,
  Trash2,
  Upload,
  X
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import type { ManifestFile, ManifestOverride, PackManifest } from "../core/manifest";
import { formatUpdateStatus, type UpdateStatus } from "../core/updater";

type InstallStatus = {
  installed: boolean;
  installedVersion: string | null;
  latestVersion: string;
  profileDir: string;
  minecraftProfileId: string;
};

type InstallSummary = {
  packId: string;
  installedVersion: string;
  profileDir: string;
  downloads: number;
  removals: number;
  minecraftProfileId: string;
};

type CurseForgeImportSummary = {
  code: string;
  packId: string;
  packName: string;
  profileDir: string;
  minecraftVersion: string;
  loaderVersion: string;
  curseforgeMods: number;
  overrides: number;
  minecraftProfileId: string;
};

type ProfileSummary = {
  code: string;
  packId: string;
  packName: string;
  version: string;
  minecraftVersion: string;
  loaderVersion: string;
  server: string;
  profileDir: string;
  installed: boolean;
  installedVersion: string | null;
  local: boolean;
};

type PublishSummary = {
  code: string;
  manifestUrl: string;
  uploadedFiles: number;
  manifest: PackManifest;
};

type FolderSyncSummary = {
  manifest: PackManifest;
  removedFiles: string[];
};

type ModrinthSearchResult = {
  projectId: string;
  title: string;
  description: string;
  downloads: number;
  iconUrl: string | null;
};

type ProgressEvent = {
  stage: string;
  message: string;
  current: number;
  total: number;
};

type ViewState = "lookup" | "ready" | "working" | "done";
type ProfileTab = "overview" | "mods";

type ManagedMod =
  | {
      kind: "file";
      file: ManifestFile;
      filename: string;
      title: string;
      detail: string;
    }
  | {
      kind: "override";
      override: ManifestOverride;
      filename: string;
      title: string;
      detail: string;
    };

const DEFAULT_CODE = "FAKERSBOB";
const DEFAULT_API_BASE = "https://launcher.ruuudy.in";

export function App() {
  const apiBase = DEFAULT_API_BASE;
  const [code, setCode] = useState(DEFAULT_CODE);
  const [adminToken, setAdminToken] = useState(() => localStorage.getItem("ruuudy-admin-token") ?? "");
  const [adminOpen, setAdminOpen] = useState(false);
  const [manifest, setManifest] = useState<PackManifest | null>(null);
  const [status, setStatus] = useState<InstallStatus | null>(null);
  const [profiles, setProfiles] = useState<ProfileSummary[]>([]);
  const [progress, setProgress] = useState<ProgressEvent | null>(null);
  const [summary, setSummary] = useState<InstallSummary | null>(null);
  const [importSummary, setImportSummary] = useState<CurseForgeImportSummary | null>(null);
  const [publishSummary, setPublishSummary] = useState<PublishSummary | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [view, setView] = useState<ViewState>("lookup");
  const [activeTab, setActiveTab] = useState<ProfileTab>("overview");
  const [modQuery, setModQuery] = useState("");
  const [modResults, setModResults] = useState<ModrinthSearchResult[]>([]);
  const [modLoading, setModLoading] = useState(false);
  const [modNotice, setModNotice] = useState<string | null>(null);
  const [launcherUpdate, setLauncherUpdate] = useState<Update | null>(null);
  const [updateStatus, setUpdateStatus] = useState<UpdateStatus>({ state: "idle" });

  useEffect(() => {
    const unlisten = listen<ProgressEvent>("install-progress", (event) => {
      setProgress(event.payload);
    });
    return () => {
      void unlisten.then((dispose) => dispose());
    };
  }, []);

  useEffect(() => {
    localStorage.setItem("ruuudy-admin-token", adminToken);
  }, [adminToken]);

  useEffect(() => {
    void refreshProfiles();
  }, []);

  useEffect(() => {
    const timer = window.setTimeout(() => {
      void checkLauncherUpdate({ silent: true });
    }, 1200);
    return () => window.clearTimeout(timer);
  }, []);

  const progressPercent = useMemo(() => {
    if (!progress || progress.total === 0) return 0;
    return Math.round((progress.current / progress.total) * 100);
  }, [progress]);

  const managedMods = useMemo<ManagedMod[]>(() => {
    if (!manifest) return [];
    return [
      ...manifest.files.map((file) => ({
        kind: "file" as const,
        file,
        filename: file.filename,
        title: displayFileModName(file),
        detail: file.filename
      })),
      ...manifest.overrides
        .filter((override) => isManagedOverrideContent(override.path))
        .map((override) => ({
          kind: "override" as const,
          override,
          filename: override.path.split(/[\\/]/).at(-1) ?? override.path,
          title: displayOverrideContentName(override),
          detail: `${override.path} - hosted override`
        }))
    ].sort((left, right) => left.title.localeCompare(right.title));
  }, [manifest]);

  async function lookupPack(nextCode = code) {
    setError(null);
    setSummary(null);
    setImportSummary(null);
    setPublishSummary(null);
    setModNotice(null);
    setProgress(null);
    try {
      let pack: PackManifest;
      try {
        pack = await invoke<PackManifest>("lookup_remote_pack", {
          code: nextCode,
          apiBase
        });
      } catch {
        pack = await invoke<PackManifest>("lookup_pack", { code: nextCode });
      }
      await loadManifest(nextCode.trim().toUpperCase(), pack);
      setActiveTab("overview");
      setView("ready");
    } catch (err) {
      setError(String(err));
      setView("lookup");
    }
  }

  async function loadManifest(nextCode: string, pack: PackManifest) {
    const nextStatus = await invoke<InstallStatus>("get_install_status", {
      manifest: pack
    });
    setManifest(pack);
    setStatus(nextStatus);
    setCode(nextCode);
  }

  async function refreshProfiles() {
    try {
      setProfiles(await invoke<ProfileSummary[]>("list_profiles"));
    } catch (err) {
      setError(String(err));
    }
  }

  async function checkLauncherUpdate(options: { silent?: boolean } = {}) {
    if (updateStatus.state === "checking" || updateStatus.state === "downloading") return;

    if (!options.silent) {
      setError(null);
    }

    setUpdateStatus({ state: "checking" });
    try {
      const update = await check();
      if (update) {
        setLauncherUpdate(update);
        setUpdateStatus({ state: "available", version: update.version });
        return;
      }

      setLauncherUpdate(null);
      setUpdateStatus(options.silent ? { state: "idle" } : { state: "current" });
    } catch (err) {
      setLauncherUpdate(null);
      if (options.silent) {
        setUpdateStatus({ state: "idle" });
        return;
      }
      setUpdateStatus({ state: "error", message: "Update check failed" });
      setError(`Launcher update check failed: ${String(err)}`);
    }
  }

  async function installLauncherUpdate() {
    setError(null);
    let update = launcherUpdate;

    if (!update) {
      setUpdateStatus({ state: "checking" });
      update = await check();
      if (!update) {
        setUpdateStatus({ state: "current" });
        return;
      }
      setLauncherUpdate(update);
      setUpdateStatus({ state: "available", version: update.version });
    }

    try {
      setUpdateStatus({ state: "downloading" });
      await update.downloadAndInstall();
      await relaunch();
    } catch (err) {
      setUpdateStatus({ state: "error", message: "Update install failed" });
      setError(`Launcher update install failed: ${String(err)}`);
    }
  }

  async function saveProfile(nextManifest: PackManifest, nextCode = code) {
    const savedCode = nextCode.trim().toUpperCase();
    await invoke<ProfileSummary>("save_profile_manifest", {
      code: savedCode,
      manifest: nextManifest
    });
    await loadManifest(savedCode, nextManifest);
    await refreshProfiles();
  }

  async function installPack() {
    if (!manifest) return;
    setError(null);
    setProgress({ stage: "start", message: "Starting install", current: 0, total: 1 });
    setView("working");
    try {
      await saveProfile(manifest);
      const result = await invoke<InstallSummary>("install_profile_pack", { code, manifest });
      const nextStatus = await invoke<InstallStatus>("get_install_status", {
        manifest
      });
      setSummary(result);
      setStatus(nextStatus);
      await refreshProfiles();
      setView("done");
    } catch (err) {
      setError(String(err));
      setView("ready");
    }
  }

  async function importCurseForgeZip() {
    setError(null);
    setProgress(null);
    const selected = await open({
      multiple: false,
      filters: [{ name: "CurseForge Modpack Zip", extensions: ["zip"] }]
    });
    if (typeof selected !== "string") return;

    setView("working");
    setProgress({ stage: "import", message: "Starting CurseForge import", current: 0, total: 1 });
    try {
      const result = await invoke<CurseForgeImportSummary>("import_curseforge_zip", {
        zipPath: selected
      });
      setImportSummary(result);
      setSummary(null);
      const pack = await invoke<PackManifest>("lookup_pack", { code: result.code });
      await loadManifest(result.code, pack);
      await refreshProfiles();
      setActiveTab("overview");
      setView("done");
    } catch (err) {
      setError(String(err));
      setView(manifest ? "ready" : "lookup");
    }
  }

  async function publishCurrentProfile() {
    if (!manifest) return;
    setError(null);
    setPublishSummary(null);
    try {
      const folderSync = await invoke<FolderSyncSummary>("sync_manifest_with_profile_folder", {
        code,
        manifest
      });
      if (folderSync.removedFiles.length > 0) {
        await loadManifest(code.trim().toUpperCase(), folderSync.manifest);
        setModNotice(
          `Removed missing local files from the shared pack: ${folderSync.removedFiles.join(", ")}.`
        );
      }
      const result = await invoke<PublishSummary>("publish_profile", {
        apiBase,
        adminToken,
        code,
        manifest: folderSync.manifest
      });
      setPublishSummary(result);
      await loadManifest(code.trim().toUpperCase(), result.manifest);
      await refreshProfiles();
    } catch (err) {
      setError(String(err));
    }
  }

  async function uploadDefaultKeybinds() {
    if (!manifest) {
      setError("Load or import a profile before uploading default keybinds.");
      return;
    }
    if (!adminToken.trim()) {
      setError("Admin token is required to upload default keybinds.");
      return;
    }

    setError(null);
    setModNotice(null);
    const selected = await open({
      multiple: false,
      filters: [{ name: "Minecraft options.txt", extensions: ["txt"] }]
    });
    if (typeof selected !== "string") return;

    try {
      const result = await invoke<PublishSummary>("upload_default_options", {
        apiBase,
        adminToken,
        code,
        manifest,
        optionsPath: selected
      });
      setPublishSummary(result);
      await loadManifest(code.trim().toUpperCase(), result.manifest);
      await refreshProfiles();
      setModNotice("Default keybinds uploaded. New installs will get them automatically.");
    } catch (err) {
      setError(String(err));
    }
  }

  async function resetDefaultKeybinds() {
    if (!manifest) return;
    if (!manifest.defaultOptions) {
      setError("This pack has no uploaded default keybinds yet.");
      return;
    }
    if (!window.confirm("Replace this profile's options.txt with the pack default keybinds?")) {
      return;
    }

    setError(null);
    try {
      await invoke("reset_default_options", { manifest });
      setModNotice("Default keybinds were applied to this profile.");
    } catch (err) {
      setError(String(err));
    }
  }

  async function deleteCurrentProfile() {
    if (!manifest) return;
    const label = `${manifest.packName} (${code})`;
    if (!window.confirm(`Delete ${label} from this PC? This removes its launcher profile and managed files.`)) {
      return;
    }
    setError(null);
    try {
      await invoke("delete_profile", { code });
      setManifest(null);
      setStatus(null);
      setSummary(null);
      setImportSummary(null);
      setPublishSummary(null);
      setModNotice(null);
      setView("lookup");
      await refreshProfiles();
    } catch (err) {
      setError(String(err));
    }
  }

  async function searchMods() {
    if (!manifest || modQuery.trim().length < 2) return;
    setError(null);
    setModNotice(null);
    setModLoading(true);
    try {
      const results = await invoke<ModrinthSearchResult[]>("search_modrinth_mods", {
        query: modQuery,
        minecraftVersion: manifest.minecraftVersion,
        loader: manifest.loader.type
      });
      setModResults(results);
    } catch (err) {
      setError(String(err));
    } finally {
      setModLoading(false);
    }
  }

  async function addMod(project: ModrinthSearchResult) {
    if (!manifest) return;
    setError(null);
    setModNotice(null);
    setModLoading(true);
    try {
      const nextManifest = await invoke<PackManifest>("add_modrinth_mod_to_profile", {
        code,
        manifest,
        projectId: project.projectId
      });
      await loadManifest(code.trim().toUpperCase(), nextManifest);
      await refreshProfiles();
      setModNotice(`${project.title} was added. Click Update Pack to download it locally, then Publish to API for friends.`);
    } catch (err) {
      setError(String(err));
    } finally {
      setModLoading(false);
    }
  }

  async function importLocalJars() {
    if (!manifest) return;
    setError(null);
    setModNotice(null);
    const selected = await open({
      multiple: true,
      filters: [{ name: "Minecraft Mod Jar", extensions: ["jar"] }]
    });
    const jarPaths = Array.isArray(selected) ? selected : typeof selected === "string" ? [selected] : [];
    if (jarPaths.length === 0) return;

    setModLoading(true);
    try {
      const nextManifest = await invoke<PackManifest>("import_local_jars_to_profile", {
        code,
        manifest,
        jarPaths
      });
      await loadManifest(code.trim().toUpperCase(), nextManifest);
      await refreshProfiles();
      setModNotice(
        `${jarPaths.length} local jar${jarPaths.length === 1 ? "" : "s"} imported. Click Publish to upload ${jarPaths.length === 1 ? "it" : "them"} for friends.`
      );
    } catch (err) {
      setError(String(err));
    } finally {
      setModLoading(false);
    }
  }

  async function importResourcePacks() {
    if (!manifest) return;
    setError(null);
    setModNotice(null);
    const selected = await open({
      multiple: true,
      filters: [{ name: "Minecraft Resource Pack Zip", extensions: ["zip"] }]
    });
    const resourcePackPaths = Array.isArray(selected) ? selected : typeof selected === "string" ? [selected] : [];
    if (resourcePackPaths.length === 0) return;

    setModLoading(true);
    try {
      const nextManifest = await invoke<PackManifest>("import_local_resource_packs_to_profile", {
        code,
        manifest,
        resourcePackPaths
      });
      await loadManifest(code.trim().toUpperCase(), nextManifest);
      await refreshProfiles();
      setModNotice(
        `${resourcePackPaths.length} resource pack${resourcePackPaths.length === 1 ? "" : "s"} imported. Click Publish to upload ${resourcePackPaths.length === 1 ? "it" : "them"} for friends.`
      );
    } catch (err) {
      setError(String(err));
    } finally {
      setModLoading(false);
    }
  }

  async function removeMod(mod: ManagedMod) {
    if (!manifest) return;
    const filename = mod.filename;
    if (!window.confirm(`Remove ${filename} from this profile manifest?`)) return;
    const nextManifest: PackManifest = {
      ...manifest,
      version: `manual-${Date.now()}`,
      files:
        mod.kind === "file"
          ? manifest.files.filter((candidate) => candidate.filename !== mod.file.filename)
          : manifest.files,
      overrides:
        mod.kind === "override"
          ? manifest.overrides.filter((candidate) => candidate.path !== mod.override.path)
          : manifest.overrides
    };
    try {
      await saveProfile(nextManifest);
      setModNotice(`${filename} was removed. Click Update Pack to remove it locally, then Publish to API for friends.`);
    } catch (err) {
      setError(String(err));
    }
  }

  async function syncFolderChanges() {
    if (!manifest) return;
    setError(null);
    setModNotice(null);
    try {
      const folderSync = await invoke<FolderSyncSummary>("sync_manifest_with_profile_folder", {
        code,
        manifest
      });
      await loadManifest(code.trim().toUpperCase(), folderSync.manifest);
      await refreshProfiles();
      if (folderSync.removedFiles.length === 0) {
        setModNotice("No missing managed mod files were found.");
      } else {
        setModNotice(
          `Removed missing local files from this profile: ${folderSync.removedFiles.join(", ")}. Publish to share that removal.`
        );
      }
    } catch (err) {
      setError(String(err));
    }
  }

  async function openFolder() {
    if (!manifest) return;
    await invoke("open_profile_folder", { manifest });
  }

  async function openOfficialLauncher() {
    await invoke("open_minecraft_launcher");
  }

  async function copyServer() {
    const server = manifest ? `${manifest.server.address}:${manifest.server.port}` : "mc.ruuudy.in:25565";
    await navigator.clipboard.writeText(server);
  }

  return (
    <main className="app-shell">
      <aside className="sidebar">
        <div className="brand">
          <div className="brand-icon">
            <Gamepad2 size={24} />
          </div>
          <div>
            <h1>Ruuudy MC</h1>
            <p>pack launcher</p>
          </div>
        </div>
        <button className="nav-button active" onClick={() => setView(manifest ? "ready" : "lookup")}>
          <PackageOpen size={18} />
          Pack
        </button>
        <div className="profile-list">
          {profiles.map((profile) => (
            <button
              key={profile.code}
              className="profile-button"
              onClick={() => void lookupPack(profile.code)}
            >
              <span>{profile.packName}</span>
              <small>{profile.code} - {profile.installed ? "ready" : "needs sync"}</small>
            </button>
          ))}
        </div>
        <button className="nav-button" onClick={importCurseForgeZip}>
          <Upload size={18} />
          Import CurseForge zip
        </button>
        <button className="nav-button" onClick={() => setAdminOpen((open) => !open)}>
          <Settings size={18} />
          Admin
        </button>
        {adminOpen && (
          <div className="admin-box">
            <label>
              <span>Admin token</span>
              <input
                value={adminToken}
                onChange={(event) => setAdminToken(event.target.value)}
                type="password"
                placeholder="Needed only to publish"
              />
            </label>
            <button
              className="secondary-button"
              onClick={() => void uploadDefaultKeybinds()}
              disabled={!manifest || !adminToken.trim()}
            >
              <Upload size={16} />
              Upload Default Keybinds
            </button>
            <p>Uses the hosted Ruuudy pack API automatically.</p>
          </div>
        )}
      </aside>

      <section className="content">
        <header className="topbar">
          <div>
            <span className="eyebrow">Server</span>
            <strong>mc.ruuudy.in</strong>
          </div>
          <div className="topbar-actions">
            <button
              className={launcherUpdate ? "update-button available" : "ghost-button"}
              onClick={() => launcherUpdate ? void installLauncherUpdate() : void checkLauncherUpdate()}
              disabled={updateStatus.state === "checking" || updateStatus.state === "downloading"}
            >
              <RefreshCcw size={16} />
              {formatUpdateStatus(updateStatus)}
            </button>
            <button className="ghost-button" onClick={copyServer}>
              <Copy size={16} />
              Copy IP
            </button>
          </div>
        </header>

        {error && (
          <div className="alert" role="alert">
            {error}
          </div>
        )}

        {view === "lookup" && (
          <section className="panel hero-panel">
            <div>
              <span className="eyebrow">Install Code</span>
              <h2>Sync the exact server pack</h2>
              <p>
                Enter a profile code or import your exported CurseForge zip. The launcher creates
                a separate Minecraft profile and keeps it updated from the shared code.
              </p>
            </div>
            <div className="code-row">
              <input
                value={code}
                onChange={(event) => setCode(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter") void lookupPack();
                }}
                placeholder="FAKERSBOB"
              />
              <button className="primary-button" onClick={() => void lookupPack()}>
                <RefreshCcw size={18} />
                Load
              </button>
            </div>
          </section>
        )}

        {manifest && view !== "lookup" && (
          <section className="panel pack-panel">
            <div className="pack-title">
              <div>
                <span className="eyebrow">Pack</span>
                <h2>{manifest.packName}</h2>
              </div>
              <StatusPill installed={Boolean(status?.installed)} />
            </div>

            <div className="tabs">
              <button className={activeTab === "overview" ? "tab active" : "tab"} onClick={() => setActiveTab("overview")}>
                Overview
              </button>
              <button className={activeTab === "mods" ? "tab active" : "tab"} onClick={() => setActiveTab("mods")}>
                Mods
              </button>
            </div>

            {activeTab === "overview" && (
              <>
                <div className="stats-grid">
                  <Stat label="Minecraft" value={manifest.minecraftVersion} />
                  <Stat label="Loader" value={`Fabric ${manifest.loader.version}`} />
                  <Stat label="Latest" value={manifest.version} />
                  <Stat label="Installed" value={status?.installedVersion ?? "Not installed"} />
                </div>
                <div className="share-panel">
                  <label>
                    <span>Share code</span>
                    <input
                      value={code}
                      onChange={(event) => setCode(event.target.value.toUpperCase())}
                      placeholder="MY-PACK"
                    />
                  </label>
                  <button className="secondary-button" onClick={() => void saveProfile(manifest)}>
                    Save Code
                  </button>
                  <button className="publish-button" onClick={publishCurrentProfile}>
                    <Send size={18} />
                    Publish
                  </button>
                </div>
                <div className="actions">
                  <button className="primary-button" onClick={installPack} disabled={view === "working"}>
                    <Download size={18} />
                    {status?.installed
                      ? "Repair / Re-sync"
                      : status?.installedVersion
                        ? "Update Pack"
                        : "Install Pack"}
                  </button>
                  <button className="secondary-button" onClick={openFolder}>
                    <FolderOpen size={18} />
                    Open Folder
                  </button>
                  <button className="secondary-button" onClick={openOfficialLauncher}>
                    <Gamepad2 size={18} />
                    Open Minecraft Launcher
                  </button>
                  {manifest.defaultOptions && (
                    <button className="secondary-button" onClick={() => void resetDefaultKeybinds()}>
                      <RefreshCcw size={18} />
                      Reset Keybinds
                    </button>
                  )}
                  <button className="danger-button" onClick={deleteCurrentProfile}>
                    <Trash2 size={18} />
                    Delete Profile
                  </button>
                </div>
              </>
            )}

            {activeTab === "mods" && (
              <section className="mod-manager">
                <div className="mod-search">
                  <label>
                    <span>Search Modrinth</span>
                    <input
                      value={modQuery}
                      onChange={(event) => setModQuery(event.target.value)}
                      onKeyDown={(event) => {
                        if (event.key === "Enter") void searchMods();
                      }}
                      placeholder={`Fabric mods for ${manifest.minecraftVersion}`}
                    />
                  </label>
                  <button className="secondary-button" onClick={searchMods} disabled={modLoading || modQuery.trim().length < 2}>
                    <Search size={18} />
                    Search
                  </button>
                  <button className="secondary-button" onClick={syncFolderChanges}>
                    <RefreshCcw size={18} />
                    Sync Folder
                  </button>
                  <button className="secondary-button" onClick={importLocalJars} disabled={modLoading}>
                    <Upload size={18} />
                    Import Jars
                  </button>
                  <button className="secondary-button" onClick={importResourcePacks} disabled={modLoading}>
                    <Upload size={18} />
                    Import Resource Packs
                  </button>
                </div>

                <div className="managed-mods">
                  <div className="section-title">
                    <span className="eyebrow">Profile content</span>
                    <strong>{managedMods.length}</strong>
                  </div>
                  {managedMods.length === 0 ? (
                    <p className="muted">No managed mods or resource packs in this profile yet.</p>
                  ) : (
                    managedMods.map((mod) => (
                      <div className="mod-row" key={`${mod.kind}-${mod.detail}`}>
                        <div>
                          <strong>{mod.title}</strong>
                          <span>{mod.detail}</span>
                        </div>
                        <button className="icon-button danger-icon" onClick={() => void removeMod(mod)} title="Remove">
                          <X size={16} />
                        </button>
                      </div>
                    ))
                  )}
                </div>

                {modNotice && <div className="notice">{modNotice}</div>}

                <div className="mod-results">
                  {modLoading && <p className="muted">Searching...</p>}
                  {!modLoading && modResults.map((result) => (
                    <div className="mod-card" key={result.projectId}>
                      {result.iconUrl ? <img src={result.iconUrl} alt="" /> : <div className="mod-icon" />}
                      <div>
                        <strong>{result.title}</strong>
                        <p>{result.description}</p>
                        <span>{result.downloads.toLocaleString()} downloads</span>
                      </div>
                      <button className="primary-button" onClick={() => void addMod(result)} disabled={modLoading}>
                        <Plus size={18} />
                        Add
                      </button>
                    </div>
                  ))}
                </div>
              </section>
            )}

            {publishSummary && (
              <div className="notice">
                Published <strong>{publishSummary.code}</strong>. Friends can enter that code to get
                the latest profile.
                {publishSummary.uploadedFiles > 0 && (
                  <> Uploaded {publishSummary.uploadedFiles} local override file{publishSummary.uploadedFiles === 1 ? "" : "s"}.</>
                )}
              </div>
            )}
          </section>
        )}

        {view === "working" && (
          <section className="panel progress-panel">
            <div className="progress-header">
              <span className="eyebrow">{progress?.stage ?? "working"}</span>
              <strong>{progressPercent}%</strong>
            </div>
            <div className="progress-track">
              <div className="progress-fill" style={{ width: `${progressPercent}%` }} />
            </div>
            <p>{progress?.message ?? "Working..."}</p>
          </section>
        )}

        {view === "done" && (summary || importSummary) && (
          <section className="panel done-panel">
            <CheckCircle2 size={34} />
            <div>
              <h2>{importSummary ? "CurseForge Zip Imported" : "Pack Ready"}</h2>
              {summary && (
                <p>
                  Installed {summary.downloads} files, removed {summary.removals}, and created the
                  official launcher profile.
                </p>
              )}
              {importSummary && (
                <p>
                  Created code <strong>{importSummary.code}</strong>, locked{" "}
                  {importSummary.curseforgeMods} CurseForge mods with SHA-256, and imported{" "}
                  {importSummary.overrides} override files.
                </p>
              )}
            </div>
            <button className="primary-button" onClick={openOfficialLauncher}>
              <Gamepad2 size={18} />
              Open Minecraft Launcher
            </button>
          </section>
        )}
      </section>
    </main>
  );
}

function StatusPill({ installed }: { installed: boolean }) {
  return <span className={installed ? "status-pill good" : "status-pill"}>{installed ? "Ready" : "Needs install"}</span>;
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div className="stat">
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}

function displayFileModName(file: ManifestFile): string {
  if (file.type === "external") return file.name;
  return file.filename.replace(/\.jar$/i, "").replace(/[-_]+/g, " ");
}

function displayOverrideContentName(override: ManifestOverride): string {
  const filename = override.path.split(/[\\/]/).at(-1) ?? override.path;
  return filename.replace(/\.(jar|zip)$/i, "").replace(/[-_]+/g, " ");
}

function isOverrideModJar(path: string): boolean {
  const normalized = path.replaceAll("\\", "/").toLowerCase();
  return normalized.startsWith("mods/") && normalized.endsWith(".jar");
}

function isResourcePackZip(path: string): boolean {
  const normalized = path.replaceAll("\\", "/").toLowerCase();
  return normalized.startsWith("resourcepacks/") && normalized.endsWith(".zip");
}

function isManagedOverrideContent(path: string): boolean {
  return isOverrideModJar(path) || isResourcePackZip(path);
}
