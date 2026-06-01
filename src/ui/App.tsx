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
  AlertTriangle,
  PackageOpen,
  Play,
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
import type { LoaderType, ManifestFile, ManifestOverride, PackManifest } from "../core/manifest";
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
  loaderType: LoaderType;
  loaderVersion: string;
  curseforgeMods: number;
  overrides: number;
  minecraftProfileId: string;
};

type CurseForgeZipPreview = {
  zipPath: string;
  packName: string;
  minecraftVersion: string;
  loaderType: LoaderType;
  loaderVersion: string;
  requiredMods: number;
  optionalMods: number;
  overrideFiles: number;
  overrideModJars: number;
  resourcePacks: number;
  shaderPacks: number;
  totalOverrideSize: number;
};

type ProfileSummary = {
  code: string;
  packId: string;
  packName: string;
  version: string;
  minecraftVersion: string;
  loaderType: LoaderType;
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

type PackHealthIssue = {
  severity: "info" | "warning" | "error";
  title: string;
  detail: string;
};

type PackHealthSummary = {
  ok: boolean;
  recommendedAction: "install" | "update" | "repair" | "play";
  issues: PackHealthIssue[];
  totalRamGib: number;
  suggestedRamGib: number;
  javaArgs: string;
  expectedFiles: number;
  missingFiles: number;
  sizeMismatches: number;
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

type ManualProfileForm = {
  code: string;
  packName: string;
  minecraftVersion: string;
  loaderType: LoaderType;
  loaderVersion: string;
  serverAddress: string;
  serverPort: string;
};

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
  const [introVisible, setIntroVisible] = useState(() => sessionStorage.getItem("ruuudy-intro-seen") !== "1");
  const [code, setCode] = useState(DEFAULT_CODE);
  const [quickCode, setQuickCode] = useState("");
  const [adminToken, setAdminToken] = useState(() => localStorage.getItem("ruuudy-admin-token") ?? "");
  const [adminOpen, setAdminOpen] = useState(false);
  const [manualOpen, setManualOpen] = useState(false);
  const [manualProfile, setManualProfile] = useState<ManualProfileForm>({
    code: "",
    packName: "",
    minecraftVersion: "1.21.1",
    loaderType: "fabric",
    loaderVersion: "0.19.2",
    serverAddress: "mc.ruuudy.in",
    serverPort: "25565"
  });
  const [manifest, setManifest] = useState<PackManifest | null>(null);
  const [status, setStatus] = useState<InstallStatus | null>(null);
  const [health, setHealth] = useState<PackHealthSummary | null>(null);
  const [profiles, setProfiles] = useState<ProfileSummary[]>([]);
  const [progress, setProgress] = useState<ProgressEvent | null>(null);
  const [summary, setSummary] = useState<InstallSummary | null>(null);
  const [importSummary, setImportSummary] = useState<CurseForgeImportSummary | null>(null);
  const [importPreview, setImportPreview] = useState<CurseForgeZipPreview | null>(null);
  const [pendingImportPath, setPendingImportPath] = useState<string | null>(null);
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
    if (!introVisible) return;

    const timer = window.setTimeout(() => {
      sessionStorage.setItem("ruuudy-intro-seen", "1");
      setIntroVisible(false);
    }, 1650);

    return () => window.clearTimeout(timer);
  }, [introVisible]);

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

  async function lookupLocalPack(nextCode: string) {
    setError(null);
    setSummary(null);
    setImportSummary(null);
    setPublishSummary(null);
    setModNotice(null);
    setProgress(null);
    try {
      const pack = await invoke<PackManifest>("lookup_pack", { code: nextCode });
      await loadManifest(nextCode.trim().toUpperCase(), pack);
      setActiveTab("overview");
      setView("ready");
    } catch (err) {
      setError(String(err));
      setView("lookup");
    }
  }

  async function lookupQuickCode() {
    const nextCode = quickCode.trim();
    if (!nextCode) {
      setView("lookup");
      return;
    }
    await lookupPack(nextCode);
    setQuickCode("");
  }

  async function loadManifest(nextCode: string, pack: PackManifest) {
    const [nextStatus, nextHealth] = await Promise.all([
      invoke<InstallStatus>("get_install_status", { manifest: pack }),
      invoke<PackHealthSummary>("get_pack_health", { manifest: pack })
    ]);
    setManifest(pack);
    setStatus(nextStatus);
    setHealth(nextHealth);
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
      setSummary(result);
      await loadManifest(code.trim().toUpperCase(), manifest);
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
    setImportPreview(null);
    setPendingImportPath(null);
    const selected = await open({
      multiple: false,
      filters: [{ name: "CurseForge Modpack Zip", extensions: ["zip"] }]
    });
    if (typeof selected !== "string") return;

    try {
      const preview = await invoke<CurseForgeZipPreview>("inspect_curseforge_zip", {
        zipPath: selected
      });
      setImportPreview(preview);
      setPendingImportPath(selected);
      setView("lookup");
    } catch (err) {
      setError(String(err));
      setView(manifest ? "ready" : "lookup");
    }
  }

  async function confirmCurseForgeImport() {
    if (!pendingImportPath) return;
    setView("working");
    setProgress({ stage: "import", message: "Starting CurseForge import", current: 0, total: 1 });
    try {
      const result = await invoke<CurseForgeImportSummary>("import_curseforge_zip", {
        zipPath: pendingImportPath
      });
      setImportPreview(null);
      setPendingImportPath(null);
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

  function cancelCurseForgeImport() {
    setImportPreview(null);
    setPendingImportPath(null);
  }

  async function createManualProfile() {
    setError(null);
    setProgress(null);
    setSummary(null);
    setImportSummary(null);
    try {
      const created = await invoke<ProfileSummary>("create_blank_profile", {
        input: {
          code: manualProfile.code,
          packName: manualProfile.packName,
          minecraftVersion: manualProfile.minecraftVersion,
          loaderType: manualProfile.loaderType,
          loaderVersion: manualProfile.loaderType === "vanilla" ? "" : manualProfile.loaderVersion,
          serverAddress: manualProfile.serverAddress,
          serverPort: Number(manualProfile.serverPort)
        }
      });
      const pack = await invoke<PackManifest>("lookup_pack", { code: created.code });
      await loadManifest(created.code, pack);
      await refreshProfiles();
      setManualOpen(false);
      setActiveTab("overview");
      setView("ready");
    } catch (err) {
      setError(String(err));
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
      setHealth(null);
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

  async function importShaderPacks() {
    if (!manifest) return;
    setError(null);
    setModNotice(null);
    const selected = await open({
      multiple: true,
      filters: [{ name: "Minecraft Shader Pack Zip", extensions: ["zip"] }]
    });
    const shaderPackPaths = Array.isArray(selected) ? selected : typeof selected === "string" ? [selected] : [];
    if (shaderPackPaths.length === 0) return;

    setModLoading(true);
    try {
      const nextManifest = await invoke<PackManifest>("import_local_shader_packs_to_profile", {
        code,
        manifest,
        shaderPackPaths
      });
      await loadManifest(code.trim().toUpperCase(), nextManifest);
      await refreshProfiles();
      setModNotice(
        `${shaderPackPaths.length} shader pack${shaderPackPaths.length === 1 ? "" : "s"} imported and enabled. Click Publish to upload ${shaderPackPaths.length === 1 ? "it" : "them"} for friends.`
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

  async function runPrimaryPackAction() {
    const action = health?.recommendedAction ?? (status?.installed ? "play" : "install");
    if (action === "play") {
      await openOfficialLauncher();
      return;
    }
    await installPack();
  }

  const primaryPackAction = health?.recommendedAction ?? (status?.installed ? "play" : "install");
  const primaryPackLabel = packActionLabel(primaryPackAction);
  const primaryPackIcon = primaryPackAction === "play" ? <Play size={18} /> : <Download size={18} />;

  return (
    <>
      {introVisible && (
        <div className="intro-splash" aria-hidden="true">
          <div className="intro-logo-wrap">
            <div className="intro-logo-mark">R</div>
          </div>
          <span>Ruuudy MC Launcher</span>
        </div>
      )}
      <main className="app-shell">
      <aside className="sidebar">
        <div className="brand">
          <div className="brand-icon r-mark" aria-hidden="true">
            <span>R</span>
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
        <div className="quick-code-box">
          <label>
            <span>Enter code</span>
            <input
              value={quickCode}
              onChange={(event) => setQuickCode(event.target.value.toUpperCase())}
              onKeyDown={(event) => {
                if (event.key === "Enter") void lookupQuickCode();
              }}
              placeholder="JUNFEET"
            />
          </label>
          <button className="secondary-button" onClick={() => void lookupQuickCode()}>
            <RefreshCcw size={16} />
            Load Code
          </button>
        </div>
        <div className="profile-list">
          {profiles.map((profile) => (
            <button
              key={profile.code}
              className="profile-button"
              onClick={() => void lookupLocalPack(profile.code)}
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
        <button className="nav-button" onClick={() => setManualOpen((open) => !open)}>
          <Plus size={18} />
          Create Empty Pack
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
              <h2>Choose a pack</h2>
              <p>
                Pick an installed pack, enter a share code, or preview a CurseForge zip before it
                starts installing.
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
            {manualOpen && (
              <div className="manual-panel">
                <label>
                  <span>Code</span>
                  <input
                    value={manualProfile.code}
                    onChange={(event) =>
                      setManualProfile((profile) => ({ ...profile, code: event.target.value.toUpperCase() }))
                    }
                    placeholder="JUNFEET"
                  />
                </label>
                <label>
                  <span>Pack name</span>
                  <input
                    value={manualProfile.packName}
                    onChange={(event) =>
                      setManualProfile((profile) => ({ ...profile, packName: event.target.value }))
                    }
                    placeholder="fakersbob"
                  />
                </label>
                <label>
                  <span>Minecraft</span>
                  <input
                    value={manualProfile.minecraftVersion}
                    onChange={(event) =>
                      setManualProfile((profile) => ({ ...profile, minecraftVersion: event.target.value }))
                    }
                    placeholder="1.21.1"
                  />
                </label>
                <label>
                  <span>Loader</span>
                  <select
                    value={manualProfile.loaderType}
                    onChange={(event) =>
                      setManualProfile((profile) => ({
                        ...profile,
                        loaderType: event.target.value as LoaderType,
                        loaderVersion: event.target.value === "vanilla" ? "" : profile.loaderVersion
                      }))
                    }
                  >
                    <option value="fabric">Fabric</option>
                    <option value="forge">Forge</option>
                    <option value="neoforge">NeoForge</option>
                    <option value="vanilla">Vanilla</option>
                  </select>
                </label>
                {manualProfile.loaderType !== "vanilla" && (
                  <label>
                    <span>Loader version</span>
                    <input
                      value={manualProfile.loaderVersion}
                      onChange={(event) =>
                        setManualProfile((profile) => ({ ...profile, loaderVersion: event.target.value }))
                      }
                      placeholder={manualProfile.loaderType === "forge" ? "52.0.0" : "0.19.2"}
                    />
                  </label>
                )}
                <label>
                  <span>Server</span>
                  <input
                    value={manualProfile.serverAddress}
                    onChange={(event) =>
                      setManualProfile((profile) => ({ ...profile, serverAddress: event.target.value }))
                    }
                    placeholder="mc.ruuudy.in"
                  />
                </label>
                <label>
                  <span>Port</span>
                  <input
                    value={manualProfile.serverPort}
                    onChange={(event) =>
                      setManualProfile((profile) => ({ ...profile, serverPort: event.target.value }))
                    }
                    placeholder="25565"
                  />
                </label>
                <button className="primary-button" onClick={() => void createManualProfile()}>
                  <Plus size={18} />
                  Create Pack
                </button>
              </div>
            )}
            {importPreview && (
              <div className="import-preview">
                <div>
                  <span className="eyebrow">CurseForge Preview</span>
                  <h3>{importPreview.packName}</h3>
                  <p>
                    {importPreview.minecraftVersion} -{" "}
                    {formatLoader(importPreview.loaderType, importPreview.loaderVersion)}
                  </p>
                </div>
                <div className="preview-grid">
                  <Stat label="Required mods" value={String(importPreview.requiredMods)} />
                  <Stat label="Optional mods" value={String(importPreview.optionalMods)} />
                  <Stat label="Overrides" value={String(importPreview.overrideFiles)} />
                  <Stat label="Resource packs" value={String(importPreview.resourcePacks)} />
                  <Stat label="Shader packs" value={String(importPreview.shaderPacks)} />
                  <Stat label="Local mod jars" value={String(importPreview.overrideModJars)} />
                  <Stat label="Override size" value={formatBytes(importPreview.totalOverrideSize)} />
                </div>
                <div className="actions">
                  <button className="primary-button" onClick={() => void confirmCurseForgeImport()}>
                    <Download size={18} />
                    Import This Pack
                  </button>
                  <button className="secondary-button" onClick={cancelCurseForgeImport}>
                    <X size={18} />
                    Cancel
                  </button>
                </div>
              </div>
            )}
            {profiles.length > 0 && (
              <div className="home-library">
                <div className="section-title">
                  <span className="eyebrow">Local packs</span>
                  <strong>{profiles.length}</strong>
                </div>
                <div className="home-pack-grid">
                  {profiles.map((profile) => (
                    <button
                      key={profile.code}
                      className="home-pack-card"
                      onClick={() => void lookupLocalPack(profile.code)}
                    >
                      <span>{profile.packName}</span>
                      <small>{profile.code}</small>
                      <em>{formatLoader(profile.loaderType, profile.loaderVersion)} - {profile.minecraftVersion}</em>
                      <strong>{profile.installed ? "Ready" : profile.installedVersion ? "Update needed" : "Install needed"}</strong>
                    </button>
                  ))}
                </div>
              </div>
            )}
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
                  <Stat label="Loader" value={formatLoader(manifest.loader.type, manifest.loader.version)} />
                  <Stat label="Latest" value={manifest.version} />
                  <Stat label="Installed" value={status?.installedVersion ?? "Not installed"} />
                </div>
                {health && (
                  <div className={health.ok ? "health-panel good" : "health-panel"}>
                    <div className="health-head">
                      <div>
                        <span className="eyebrow">Pack Health</span>
                        <strong>{health.ok ? "Ready to play" : `${health.issues.length} thing${health.issues.length === 1 ? "" : "s"} to check`}</strong>
                      </div>
                      <div className="ram-pill">
                        <span>{health.totalRamGib} GiB PC</span>
                        <strong>{health.suggestedRamGib}G client RAM</strong>
                      </div>
                    </div>
                    <div className="health-metrics">
                      <span>{health.expectedFiles} managed files</span>
                      <span>{health.missingFiles} missing</span>
                      <span>{health.sizeMismatches} size mismatches</span>
                      <span>{health.javaArgs}</span>
                    </div>
                    {health.issues.length > 0 && (
                      <div className="health-issues">
                        {health.issues.slice(0, 5).map((issue) => (
                          <div className={`health-issue ${issue.severity}`} key={`${issue.title}-${issue.detail}`}>
                            <AlertTriangle size={16} />
                            <div>
                              <strong>{issue.title}</strong>
                              <span>{issue.detail}</span>
                            </div>
                          </div>
                        ))}
                        {health.issues.length > 5 && (
                          <p className="muted">+{health.issues.length - 5} more issue{health.issues.length - 5 === 1 ? "" : "s"}</p>
                        )}
                      </div>
                    )}
                  </div>
                )}
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
                  <button className="primary-button" onClick={() => void runPrimaryPackAction()} disabled={view === "working"}>
                    {primaryPackIcon}
                    {primaryPackLabel}
                  </button>
                  {primaryPackAction === "play" && (
                    <button className="secondary-button" onClick={installPack} disabled={view === "working"}>
                      <RefreshCcw size={18} />
                      Repair / Re-sync
                    </button>
                  )}
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
                      placeholder={`${formatLoader(manifest.loader.type, "").trim()} mods for ${manifest.minecraftVersion}`}
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
                  <button className="secondary-button" onClick={importShaderPacks} disabled={modLoading}>
                    <Upload size={18} />
                    Import Shader Packs
                  </button>
                </div>

                <div className="managed-mods">
                  <div className="section-title">
                    <span className="eyebrow">Profile content</span>
                    <strong>{managedMods.length}</strong>
                  </div>
                  {managedMods.length === 0 ? (
                    <p className="muted">No managed mods, resource packs, or shader packs in this profile yet.</p>
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
                  Created code <strong>{importSummary.code}</strong> for{" "}
                  {formatLoader(importSummary.loaderType, importSummary.loaderVersion)}, locked{" "}
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
    </>
  );
}

function StatusPill({ installed }: { installed: boolean }) {
  return <span className={installed ? "status-pill good" : "status-pill"}>{installed ? "Ready" : "Needs install"}</span>;
}

function packActionLabel(action: PackHealthSummary["recommendedAction"]): string {
  switch (action) {
    case "play":
      return "Play";
    case "update":
      return "Update Pack";
    case "repair":
      return "Repair Pack";
    case "install":
      return "Install Pack";
  }
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div className="stat">
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}

function formatLoader(loaderType: LoaderType, loaderVersion: string): string {
  const name = {
    vanilla: "Vanilla",
    fabric: "Fabric",
    forge: "Forge",
    neoforge: "NeoForge"
  }[loaderType];
  return loaderType === "vanilla" ? name : `${name} ${loaderVersion}`;
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const kib = bytes / 1024;
  if (kib < 1024) return `${kib.toFixed(1)} KiB`;
  const mib = kib / 1024;
  if (mib < 1024) return `${mib.toFixed(1)} MiB`;
  return `${(mib / 1024).toFixed(2)} GiB`;
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

function isShaderPackZip(path: string): boolean {
  const normalized = path.replaceAll("\\", "/").toLowerCase();
  return normalized.startsWith("shaderpacks/") && normalized.endsWith(".zip");
}

function isManagedOverrideContent(path: string): boolean {
  return isOverrideModJar(path) || isResourcePackZip(path) || isShaderPackZip(path);
}
