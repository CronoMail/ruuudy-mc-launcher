import { describe, expect, it } from "vitest";
import {
  buildInstallPlan,
  normalizePackCode,
  type LocalInstallState,
  type PackManifest
} from "./manifest";
import {
  createOrUpdateLauncherProfile,
  type LauncherProfilesFile
} from "./minecraftProfile";

const manifest: PackManifest = {
  schemaVersion: 1,
  packId: "fakersbob",
  packName: "Fakersbob",
  version: "2026.05.22-1",
  minecraftVersion: "1.21.1",
  loader: {
    type: "fabric",
    version: "0.19.2"
  },
  server: {
    address: "mc.ruuudy.in",
    port: 25565
  },
  files: [
    {
      type: "modrinth",
      side: "both",
      required: true,
      projectId: "P7dR8mSH",
      versionId: "abc123",
      filename: "fabric-api.jar",
      sha512: "f".repeat(128),
      size: 10
    },
    {
      type: "external",
      side: "client",
      required: true,
      name: "Manual Mod",
      filename: "manual-mod.jar",
      url: "https://example.com/manual-mod.jar",
      sha256: "a".repeat(64),
      size: 20
    }
  ],
  overrides: [
    {
      path: "config/example.toml",
      url: "https://launcher.ruuudy.in/files/config/example.toml",
      sha256: "b".repeat(64),
      size: 30
    }
  ]
};

describe("normalizePackCode", () => {
  it("trims whitespace and uppercases pack codes", () => {
    expect(normalizePackCode("  fakersbob \n")).toBe("FAKERSBOB");
  });

  it("rejects unsafe pack code characters", () => {
    expect(() => normalizePackCode("../fakersbob")).toThrow(/letters, numbers, dashes, and underscores/i);
  });
});

describe("buildInstallPlan", () => {
  it("keeps the desktop install plan unchanged when optional server metadata is present", () => {
    const withServerMetadata: PackManifest = {
      ...manifest,
      serverPack: {
        enabled: true,
        preservePaths: ["world/**", "server.properties"]
      },
      overrides: manifest.overrides.map((override) => ({
        ...override,
        side: "both"
      }))
    };

    expect(
      buildInstallPlan(withServerMetadata, null, { existingFiles: [] })
    ).toEqual(buildInstallPlan(manifest, null, { existingFiles: [] }));
  });

  it("downloads missing manifest files and removes only previously managed stale files", () => {
    const localState: LocalInstallState = {
      packId: "fakersbob",
      manifestVersion: "old",
      managedFiles: [
        "mods/old-managed.jar",
        "config/old-managed.toml"
      ]
    };

    const plan = buildInstallPlan(manifest, localState, {
      existingFiles: [
        "mods/user-owned.jar",
        "mods/old-managed.jar",
        "config/old-managed.toml"
      ]
    });

    expect(plan.downloads.map((item) => item.relativePath)).toEqual([
      "mods/fabric-api.jar",
      "mods/manual-mod.jar",
      "config/example.toml"
    ]);
    expect(plan.removals).toEqual([
      "mods/old-managed.jar",
      "config/old-managed.toml"
    ]);
    expect(plan.nextManagedFiles).toEqual([
      "mods/fabric-api.jar",
      "mods/manual-mod.jar",
      "config/example.toml"
    ]);
  });

  it("refuses external files without hashes", () => {
    const unsafeManifest: PackManifest = {
      ...manifest,
      files: [
        {
          type: "external",
          side: "client",
          required: true,
          name: "Unsafe",
          filename: "unsafe.jar",
          url: "https://example.com/unsafe.jar",
          sha256: "",
          size: 1
        }
      ],
      overrides: []
    };

    expect(() =>
      buildInstallPlan(unsafeManifest, null, { existingFiles: [] })
    ).toThrow(/external.*sha-256/i);
  });

  it("applies default options only on first install", () => {
    const manifestWithDefaults: PackManifest = {
      ...manifest,
      defaultOptions: {
        path: "options.txt",
        url: "https://launcher.ruuudy.in/files/defaults/options.txt",
        sha256: "c".repeat(64),
        size: 40
      }
    };

    const firstInstall = buildInstallPlan(manifestWithDefaults, null, {
      existingFiles: []
    });
    const resync = buildInstallPlan(
      manifestWithDefaults,
      {
        packId: "fakersbob",
        manifestVersion: "2026.05.22-1",
        managedFiles: [
          "mods/fabric-api.jar",
          "mods/manual-mod.jar",
          "config/example.toml"
        ]
      },
      {
        existingFiles: ["options.txt"]
      }
    );

    expect(firstInstall.downloads.map((item) => item.relativePath)).toContain("options.txt");
    expect(firstInstall.nextManagedFiles).not.toContain("options.txt");
    expect(resync.downloads.map((item) => item.relativePath)).not.toContain("options.txt");
    expect(resync.removals).toEqual([]);
  });
});

describe("createOrUpdateLauncherProfile", () => {
  it("adds a dedicated official launcher profile without removing existing profiles", () => {
    const profiles: LauncherProfilesFile = {
      profiles: {
        vanilla: {
          name: "Vanilla",
          type: "custom",
          created: "2026-01-01T00:00:00.000Z",
          lastUsed: "2026-01-01T00:00:00.000Z",
          lastVersionId: "1.21.1"
        }
      },
      settings: {}
    };

    const updated = createOrUpdateLauncherProfile(profiles, {
      profileId: "ruuudy-fakersbob",
      profileName: "Fakersbob Server",
      gameDir: "C:\\Users\\Rudy\\AppData\\Roaming\\.ruuudy-mc\\profiles\\fakersbob",
      minecraftVersion: "1.21.1",
      loaderVersion: "0.19.2"
    });

    expect(Object.keys(updated.profiles)).toEqual(["vanilla", "ruuudy-fakersbob"]);
    expect(updated.profiles["ruuudy-fakersbob"]).toMatchObject({
      name: "Fakersbob Server",
      type: "custom",
      gameDir: "C:\\Users\\Rudy\\AppData\\Roaming\\.ruuudy-mc\\profiles\\fakersbob",
      lastVersionId: "fabric-loader-0.19.2-1.21.1"
    });
    expect(updated.profiles.vanilla.name).toBe("Vanilla");
  });
});
