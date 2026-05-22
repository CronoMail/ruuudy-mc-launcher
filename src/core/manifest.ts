export type LoaderType = "fabric";
export type FileSide = "client" | "server" | "both";

export type PackManifest = {
  schemaVersion: 1;
  packId: string;
  packName: string;
  version: string;
  minecraftVersion: string;
  loader: {
    type: LoaderType;
    version: string;
  };
  server: {
    address: string;
    port: number;
  };
  files: ManifestFile[];
  overrides: ManifestOverride[];
};

export type ModrinthManifestFile = {
  type: "modrinth";
  side: FileSide;
  required: boolean;
  projectId: string;
  versionId: string;
  filename: string;
  sha512: string;
  size?: number;
};

export type ExternalManifestFile = {
  type: "external";
  side: FileSide;
  required: boolean;
  name: string;
  filename: string;
  url: string;
  sha256: string;
  size: number;
};

export type ManifestFile = ModrinthManifestFile | ExternalManifestFile;

export type ManifestOverride = {
  path: string;
  url: string;
  sha256: string;
  size: number;
};

export type LocalInstallState = {
  packId: string;
  manifestVersion: string;
  managedFiles: string[];
};

export type DownloadPlanItem = {
  relativePath: string;
  url: string | null;
  filename: string;
  expectedHash: {
    algorithm: "sha256" | "sha512";
    value: string;
  };
  size?: number;
  source: "modrinth" | "external" | "override";
};

export type InstallPlan = {
  downloads: DownloadPlanItem[];
  removals: string[];
  nextManagedFiles: string[];
};

const SAFE_CODE_PATTERN = /^[A-Z0-9_-]{2,32}$/;

export function normalizePackCode(code: string): string {
  const normalized = code.trim().toUpperCase();
  if (!SAFE_CODE_PATTERN.test(normalized)) {
    throw new Error("Pack codes may only contain letters, numbers, dashes, and underscores.");
  }
  return normalized;
}

export function buildInstallPlan(
  manifest: PackManifest,
  localState: LocalInstallState | null,
  _environment: { existingFiles: string[] }
): InstallPlan {
  validateManifest(manifest);

  const downloads = [
    ...manifest.files.map(fileToDownload),
    ...manifest.overrides.map(overrideToDownload)
  ];
  const nextManagedFiles = downloads.map((item) => item.relativePath);
  const nextManagedSet = new Set(nextManagedFiles);
  const removals = (localState?.managedFiles ?? []).filter(
    (relativePath) => !nextManagedSet.has(relativePath)
  );

  return {
    downloads,
    removals,
    nextManagedFiles
  };
}

function validateManifest(manifest: PackManifest): void {
  if (manifest.schemaVersion !== 1) {
    throw new Error(`Unsupported manifest schema ${manifest.schemaVersion}.`);
  }

  for (const file of manifest.files) {
    if (file.type === "external" && !/^[a-f0-9]{64}$/i.test(file.sha256)) {
      throw new Error(`External file ${file.filename} must include a SHA-256 hash.`);
    }

    if (file.type === "modrinth" && !/^[a-f0-9]{128}$/i.test(file.sha512)) {
      throw new Error(`Modrinth file ${file.filename} must include a SHA-512 hash.`);
    }
  }

  for (const override of manifest.overrides) {
    if (!/^[a-f0-9]{64}$/i.test(override.sha256)) {
      throw new Error(`Override ${override.path} must include a SHA-256 hash.`);
    }
  }
}

function fileToDownload(file: ManifestFile): DownloadPlanItem {
  if (file.type === "modrinth") {
    return {
      relativePath: `mods/${file.filename}`,
      url: null,
      filename: file.filename,
      expectedHash: {
        algorithm: "sha512",
        value: file.sha512
      },
      size: file.size,
      source: "modrinth"
    };
  }

  return {
    relativePath: `mods/${file.filename}`,
    url: file.url,
    filename: file.filename,
    expectedHash: {
      algorithm: "sha256",
      value: file.sha256
    },
    size: file.size,
    source: "external"
  };
}

function overrideToDownload(override: ManifestOverride): DownloadPlanItem {
  return {
    relativePath: override.path.replaceAll("\\", "/"),
    url: override.url,
    filename: override.path.split(/[\\/]/).at(-1) ?? override.path,
    expectedHash: {
      algorithm: "sha256",
      value: override.sha256
    },
    size: override.size,
    source: "override"
  };
}
