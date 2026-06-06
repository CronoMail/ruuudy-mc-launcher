import { createHash, randomUUID } from "node:crypto";
import {
  cp,
  mkdir,
  readFile,
  readdir,
  rename,
  rm,
  stat,
  writeFile
} from "node:fs/promises";
import { basename, dirname, join, relative } from "node:path";
import { resolveInside, resolveServerPath, safeRelativePath } from "./paths.mjs";

const STATE_FILE = ".ruuudy-pack-install.json";
const RUNTIME_PRESERVE_PATHS = [
  "libraries/**",
  "versions/**",
  "unix_args.txt",
  "user_jvm_args.txt",
  "server.jar",
  "fabric-server-launch.jar",
  "forge-*.jar",
  "neoforge-*.jar",
  "eula.txt"
];
const DEFAULT_PRESERVE_PATHS = [
  "world/**",
  "world_nether/**",
  "world_the_end/**",
  "server.properties",
  "ops.json",
  "whitelist.json",
  "banned-players.json",
  "banned-ips.json",
  "permissions.json",
  "usercache.json"
];

export async function installFromManifest({
  serversRoot,
  serverId,
  manifest,
  mode,
  preservePaths = [],
  fetchImpl = fetch,
  onProgress = () => {}
}) {
  if (!["preserve", "wipe"].includes(mode)) {
    throw new Error("Install mode must be preserve or wipe.");
  }
  validateServerManifest(manifest);

  const livePath = resolveServerPath(serversRoot, serverId);
  const operationRoot = resolveInside(serversRoot, `.ruuudy-pack-installer/${serverId}`);
  const jobId = `${Date.now()}-${randomUUID()}`;
  const stagePath = join(operationRoot, "staging", jobId);
  const rollbackPath = join(operationRoot, "rollbacks", jobId);
  await mkdir(stagePath, { recursive: true });

  try {
    let completed = 0;
    for (const file of manifest.files) {
      onProgress({ phase: "downloading", completed, total: manifest.files.length, path: file.path });
      await downloadFile(file, stagePath, fetchImpl);
      completed += 1;
    }

    await writeFile(
      join(stagePath, STATE_FILE),
      `${JSON.stringify(
        {
          schemaVersion: 1,
          code: manifest.code,
          packId: manifest.packId,
          packName: manifest.packName,
          version: manifest.version,
          minecraftVersion: manifest.minecraftVersion,
          loader: manifest.loader,
          managedFiles: manifest.files.map((file) => safeRelativePath(file.path)),
          mode,
          installedAt: new Date().toISOString()
        },
        null,
        2
      )}\n`
    );

    onProgress({ phase: "finalizing", completed, total: manifest.files.length });
    await mkdir(dirname(rollbackPath), { recursive: true });
    await rename(livePath, rollbackPath);
    try {
      await mkdir(livePath, { recursive: true });
      await copyPreservedPaths(rollbackPath, livePath, [
        ...RUNTIME_PRESERVE_PATHS,
        ...(mode === "preserve"
          ? [...DEFAULT_PRESERVE_PATHS, ...(manifest.preservePaths ?? []), ...preservePaths]
          : [])
      ]);
      await cp(stagePath, livePath, { recursive: true, force: true });
    } catch (error) {
      await rm(livePath, { recursive: true, force: true });
      await rename(rollbackPath, livePath);
      throw error;
    }

    await rm(stagePath, { recursive: true, force: true });
    onProgress({ phase: "completed", completed, total: manifest.files.length });
    return {
      jobId,
      serverId,
      mode,
      rollbackPath,
      installedVersion: manifest.version,
      managedFiles: manifest.files.length
    };
  } catch (error) {
    await rm(stagePath, { recursive: true, force: true });
    throw error;
  }
}

export async function rollbackInstallation({ serversRoot, serverId, rollbackPath }) {
  const livePath = resolveServerPath(serversRoot, serverId);
  const rollbacksRoot = resolveInside(
    serversRoot,
    `.ruuudy-pack-installer/${serverId}/rollbacks`
  );
  const safeRollback = resolveInside(rollbacksRoot, basename(rollbackPath));
  await stat(safeRollback);

  const replacedPath = resolveInside(
    serversRoot,
    `.ruuudy-pack-installer/${serverId}/replaced/${Date.now()}`
  );
  await mkdir(dirname(replacedPath), { recursive: true });
  await rename(livePath, replacedPath);
  try {
    await rename(safeRollback, livePath);
  } catch (error) {
    await rename(replacedPath, livePath);
    throw error;
  }
  return { serverId, restoredFrom: safeRollback, replacedPath };
}

async function downloadFile(file, stagePath, fetchImpl) {
  const relativePath = safeRelativePath(file.path);
  const target = join(stagePath, relativePath);
  await mkdir(dirname(target), { recursive: true });
  const url = file.type === "modrinth" ? await resolveModrinthUrl(file, fetchImpl) : file.url;
  if (typeof url !== "string" || url.length === 0) {
    throw new Error(`No download URL for ${relativePath}.`);
  }

  const response = await fetchImpl(url);
  if (!response.ok) {
    throw new Error(`Download failed for ${relativePath}: HTTP ${response.status}.`);
  }
  const bytes = Buffer.from(await response.arrayBuffer());
  if (file.size !== undefined && file.size !== null && bytes.length !== file.size) {
    throw new Error(`Size mismatch for ${relativePath}. Expected ${file.size}, got ${bytes.length}.`);
  }
  const actualHash = createHash(file.hash.algorithm).update(bytes).digest("hex");
  if (actualHash.toLowerCase() !== file.hash.value.toLowerCase()) {
    throw new Error(`Hash mismatch for ${relativePath}.`);
  }
  await writeFile(target, bytes);
}

async function resolveModrinthUrl(file, fetchImpl) {
  const response = await fetchImpl(`https://api.modrinth.com/v2/version/${file.versionId}`);
  if (!response.ok) {
    throw new Error(`Could not resolve Modrinth version ${file.versionId}.`);
  }
  const version = await response.json();
  const selected =
    version.files?.find((entry) => entry.filename === basename(file.path)) ??
    version.files?.find((entry) => entry.primary) ??
    version.files?.[0];
  if (!selected?.url) {
    throw new Error(`Modrinth version ${file.versionId} has no downloadable file.`);
  }
  return selected.url;
}

async function copyPreservedPaths(sourceRoot, targetRoot, patterns) {
  for (const source of await walkFiles(sourceRoot)) {
    const relativePath = relative(sourceRoot, source).replaceAll("\\", "/");
    if (!patterns.some((pattern) => matchesPattern(relativePath, pattern))) {
      continue;
    }
    const target = join(targetRoot, relativePath);
    await mkdir(dirname(target), { recursive: true });
    await cp(source, target, { force: true });
  }
}

async function walkFiles(root) {
  const files = [];
  for (const entry of await readdir(root, { withFileTypes: true })) {
    const path = join(root, entry.name);
    if (entry.isDirectory()) {
      files.push(...(await walkFiles(path)));
    } else if (entry.isFile()) {
      files.push(path);
    }
  }
  return files;
}

function matchesPattern(path, pattern) {
  const normalized = pattern.replaceAll("\\", "/").replace(/^\/+/, "");
  if (normalized.endsWith("/**")) {
    const prefix = normalized.slice(0, -3);
    return path === prefix || path.startsWith(`${prefix}/`);
  }
  const escaped = normalized.replace(/[.+^${}()|[\]\\]/g, "\\$&").replaceAll("*", "[^/]*");
  return new RegExp(`^${escaped}$`).test(path);
}

function validateServerManifest(manifest) {
  if (manifest?.schemaVersion !== 1 || !Array.isArray(manifest.files)) {
    throw new Error("Unsupported server manifest.");
  }
  for (const file of manifest.files) {
    safeRelativePath(file.path);
    if (!["sha256", "sha512"].includes(file.hash?.algorithm)) {
      throw new Error(`Unsupported hash algorithm for ${file.path}.`);
    }
    if (typeof file.hash.value !== "string" || file.hash.value.length === 0) {
      throw new Error(`Missing hash for ${file.path}.`);
    }
  }
}
