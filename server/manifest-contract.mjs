const DISTRIBUTION_SIDES = new Set(["both", "client", "server", "excluded"]);
const SERVER_SIDES = new Set(["both", "server"]);

export function validateDistributionMetadata(manifest) {
  for (const file of manifest.files ?? []) {
    validateSide(file.side, `File ${file.filename ?? "unknown"}`);
  }

  for (const override of manifest.overrides ?? []) {
    validateSide(override.side ?? "both", `Override ${override.path ?? "unknown"}`);
    safeRelativePath(override.path);
  }

  if (manifest.serverPack !== undefined) {
    if (typeof manifest.serverPack !== "object" || manifest.serverPack === null) {
      throw httpError(400, "serverPack must be an object.");
    }
    if (typeof manifest.serverPack.enabled !== "boolean") {
      throw httpError(400, "serverPack.enabled must be a boolean.");
    }
    if (
      manifest.serverPack.preservePaths !== undefined &&
      (!Array.isArray(manifest.serverPack.preservePaths) ||
        manifest.serverPack.preservePaths.some((path) => typeof path !== "string" || path.trim() === ""))
    ) {
      throw httpError(400, "serverPack.preservePaths must contain non-empty strings.");
    }
  }
}

export function buildServerManifest(manifest, { origin, code }) {
  validateDistributionMetadata(manifest);
  if (manifest.serverPack?.enabled !== true) {
    throw httpError(409, `Pack ${code} is not enabled for server installation.`);
  }

  const files = [];
  for (const file of manifest.files) {
    if (!SERVER_SIDES.has(file.side)) {
      continue;
    }
    files.push(normalizeManifestFile(file, origin));
  }

  for (const override of manifest.overrides) {
    if (!SERVER_SIDES.has(override.side ?? "both")) {
      continue;
    }
    files.push({
      type: "override",
      path: safeRelativePath(override.path),
      url: absoluteUrl(override.url, origin),
      size: override.size,
      hash: { algorithm: "sha256", value: override.sha256 }
    });
  }

  return {
    schemaVersion: 1,
    code,
    packId: manifest.packId,
    packName: manifest.packName,
    version: manifest.version,
    minecraftVersion: manifest.minecraftVersion,
    loader: manifest.loader,
    preservePaths: [...(manifest.serverPack.preservePaths ?? [])],
    files
  };
}

function normalizeManifestFile(file, origin) {
  const common = {
    type: file.type,
    path: safeRelativePath(`mods/${file.filename}`),
    size: file.size,
    side: file.side
  };

  if (file.type === "modrinth") {
    return {
      ...common,
      projectId: file.projectId,
      versionId: file.versionId,
      hash: { algorithm: "sha512", value: file.sha512 }
    };
  }

  return {
    ...common,
    url: absoluteUrl(file.url, origin),
    hash: { algorithm: "sha256", value: file.sha256 }
  };
}

function validateSide(side, label) {
  if (!DISTRIBUTION_SIDES.has(side)) {
    throw httpError(400, `${label} has invalid side '${side}'.`);
  }
}

function absoluteUrl(value, origin) {
  return new URL(value, origin).toString();
}

function safeRelativePath(value) {
  if (typeof value !== "string") {
    throw httpError(400, "Manifest file path must be a string.");
  }
  const normalized = value.replaceAll("\\", "/");
  const parts = normalized.split("/").filter(Boolean);
  if (
    parts.length === 0 ||
    normalized.startsWith("/") ||
    parts.some((part) => part === "." || part === ".." || part.includes("\0"))
  ) {
    throw httpError(400, `Unsafe manifest file path '${value}'.`);
  }
  return parts.join("/");
}

function httpError(statusCode, message) {
  const error = new Error(message);
  error.statusCode = statusCode;
  return error;
}

