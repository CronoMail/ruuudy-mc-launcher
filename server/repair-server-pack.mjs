import { readFile, writeFile } from "node:fs/promises";
import { join, normalize } from "node:path";
import {
  buildServerManifest,
  validateDistributionMetadata
} from "./manifest-contract.mjs";

const DATA_DIR = process.env.PACK_DATA_DIR ?? join(import.meta.dirname, "data");
const DEFAULT_PRESERVE_PATHS = [
  "world/**",
  "server.properties",
  "ops.json",
  "whitelist.json",
  "banned-players.json",
  "banned-ips.json"
];

const code = normalizeCode(process.argv[2] ?? "");
if (!code) {
  console.error("Usage: node server/repair-server-pack.mjs <PACK_CODE> [--dry-run]");
  process.exit(1);
}

const dryRun = process.argv.includes("--dry-run");
const manifestPath = normalize(join(DATA_DIR, "packs", code, "manifest.json"));
const manifest = JSON.parse(await readFile(manifestPath, "utf8"));
const before = summarize(manifest);

manifest.serverPack = {
  enabled: true,
  preservePaths: [
    ...new Set([...(manifest.serverPack?.preservePaths ?? []), ...DEFAULT_PRESERVE_PATHS])
  ]
};

for (const file of manifest.files ?? []) {
  if (isKnownClientOnlyFile(file)) {
    file.side = "client";
  } else if (file.side === "client" && shouldPromoteFileToServer(file)) {
    file.side = "both";
  }
}

for (const override of manifest.overrides ?? []) {
  if (shouldMarkOverrideClientOnly(override.path)) {
    override.side = "client";
  }
}

validateDistributionMetadata(manifest);
const serverManifest = buildServerManifest(manifest, {
  origin: "https://launcher.ruuudy.in",
  code
});

if (!dryRun) {
  await writeFile(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`, "utf8");
}

const after = summarize(manifest);
console.log(
  JSON.stringify(
    {
      code,
      dryRun,
      before,
      after,
      serverFiles: serverManifest.files.length,
      preservePaths: serverManifest.preservePaths
    },
    null,
    2
  )
);

function normalizeCode(value) {
  const normalized = value.trim().toUpperCase();
  if (!/^[A-Z0-9_-]{2,32}$/.test(normalized)) {
    return "";
  }
  return normalized;
}

function shouldPromoteFileToServer(file) {
  if (file.type !== "external" && file.type !== "modrinth") {
    return false;
  }

  const text = [
    file.filename ?? "",
    file.name ?? "",
    file.url ?? "",
    file.projectId ?? "",
    file.versionId ?? ""
  ]
    .join(" ")
    .toLowerCase();

  return !isKnownClientOnlyText(text);
}

function isKnownClientOnlyFile(file) {
  const text = [
    file.filename ?? "",
    file.name ?? "",
    file.url ?? "",
    file.projectId ?? "",
    file.versionId ?? ""
  ]
    .join(" ")
    .toLowerCase();
  return isKnownClientOnlyText(text);
}

function isKnownClientOnlyText(text) {
  return [
    "resourcepack",
    "resource-pack",
    "shaderpack",
    "shader-pack",
    "optifine",
    "iris",
    "oculus",
    "embeddium",
    "rubidium",
    "sodium",
    "xaero",
    "journeymap",
    "jeiintegration",
    "jade",
    "wthit",
    "mouse tweaks",
    "sound physics",
    "chat heads",
    "fallingleaves",
    "legendarytooltips",
    "legendary tooltips",
    "colorwheel",
    "continuity",
    "better third person",
    "entity texture features",
    "entity_texture_features",
    "entity model features",
    "entity_model_features",
    "visual keybinder",
    "visual_keybinder",
    "fancymenu",
    "bhmenu",
    "drippyloadingscreen",
    "inventoryhud",
    "notenoughanimations",
    "screenshot_viewer",
    "screenshot viewer",
    "mousetweaks",
    "mouse tweaks",
    "controlling",
    "searchables",
    "catalogue",
    "configured",
    "clean_tooltips",
    "clean tooltips",
    "clientcrafting",
    "foodeffecttooltips",
    "konkrete",
    "watermedia",
    "shouldersurfing",
    "shoulder surfing",
    "tp_shooting",
    "tp-shooting",
    "blur-forge",
    "craftpresence"
  ].some((marker) => text.includes(marker));
}

function shouldMarkOverrideClientOnly(path) {
  const normalized = String(path ?? "").replaceAll("\\", "/").toLowerCase();
  return (
    normalized === "options.txt" ||
    normalized.startsWith("resourcepacks/") ||
    normalized.startsWith("shaderpacks/") ||
    normalized.startsWith("screenshots/") ||
    (normalized.startsWith("mods/") && isKnownClientOnlyText(normalized))
  );
}

function summarize(manifest) {
  const files = manifest.files ?? [];
  const overrides = manifest.overrides ?? [];
  return {
    serverPackEnabled: manifest.serverPack?.enabled === true,
    files: files.length,
    clientFiles: files.filter((file) => file.side === "client").length,
    bothFiles: files.filter((file) => file.side === "both").length,
    serverFiles: files.filter((file) => file.side === "server").length,
    overrides: overrides.length,
    clientOverrides: overrides.filter((override) => override.side === "client").length
  };
}
