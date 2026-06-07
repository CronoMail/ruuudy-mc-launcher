import { describe, expect, it } from "vitest";
import {
  buildServerManifest,
  validateDistributionMetadata
} from "./manifest-contract.mjs";

const baseManifest = {
  schemaVersion: 1,
  packId: "example",
  packName: "Example",
  version: "1.0.0",
  minecraftVersion: "1.20.1",
  loader: { type: "forge", version: "47.4.0" },
  server: { address: "mc.example.test", port: 25565 },
  serverPack: {
    enabled: true,
    preservePaths: ["world/**", "server.properties"]
  },
  files: [
    {
      type: "external",
      side: "both",
      required: true,
      name: "Shared",
      filename: "shared.jar",
      url: "https://cdn.example.test/shared.jar",
      sha256: "a".repeat(64),
      size: 10
    },
    {
      type: "external",
      side: "client",
      required: true,
      name: "Client",
      filename: "client.jar",
      url: "https://cdn.example.test/client.jar",
      sha256: "b".repeat(64),
      size: 20
    }
  ],
  overrides: [
    {
      path: "config/shared.toml",
      url: "/api/packs/EXAMPLE/files/config/shared.toml",
      sha256: "c".repeat(64),
      size: 30
    },
    {
      path: "resourcepacks/client.zip",
      url: "/api/packs/EXAMPLE/files/resourcepacks/client.zip",
      sha256: "d".repeat(64),
      size: 40,
      side: "client"
    }
  ],
  defaultOptions: {
    path: "options.txt",
    url: "/api/packs/EXAMPLE/files/options.txt",
    sha256: "e".repeat(64),
    size: 50
  }
};

describe("validateDistributionMetadata", () => {
  it("accepts omitted override side as both", () => {
    expect(() => validateDistributionMetadata(baseManifest)).not.toThrow();
  });

  it("rejects invalid side values", () => {
    expect(() =>
      validateDistributionMetadata({
        ...baseManifest,
        overrides: [{ ...baseManifest.overrides[0], side: "desktop" }]
      })
    ).toThrow(/side/i);
  });
});


describe("buildServerManifest", () => {
  it("filters client-only content and defaults override side to both", () => {
    const result = buildServerManifest(baseManifest, {
      origin: "https://launcher.example.test",
      code: "EXAMPLE"
    });

    expect(result.files.map((file: { path: string }) => file.path)).toEqual([
      "mods/shared.jar",
      "config/shared.toml"
    ]);
    expect(result.files[1].url).toBe(
      "https://launcher.example.test/api/packs/EXAMPLE/files/config/shared.toml"
    );
    expect(result.preservePaths).toEqual(["world/**", "server.properties"]);
    expect(JSON.stringify(result)).not.toContain("options.txt");
  });

  it("refuses packs that are not explicitly enabled for servers", () => {
    expect(() =>
      buildServerManifest(
        { ...baseManifest, serverPack: undefined },
        { origin: "https://launcher.example.test", code: "EXAMPLE" }
      )
    ).toThrow(/not enabled/i);
  });
});
