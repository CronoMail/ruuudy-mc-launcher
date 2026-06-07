import { createHash } from "node:crypto";
import { lstat, mkdtemp, mkdir, readFile, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import {
  installFromManifest,
  rollbackInstallation
} from "../src/installer.mjs";
import { resolveServerPath } from "../src/paths.mjs";

const SERVER_ID = "34e80249-582d-47fe-9680-8dd3a64411cc";

function sha256(value: string) {
  return createHash("sha256").update(value).digest("hex");
}

function fixtureManifest(contents: Record<string, string>) {
  return {
    schemaVersion: 1,
    code: "BIOHAZARD",
    packId: "biohazard",
    packName: "BioHazard",
    version: "1.0.0",
    minecraftVersion: "1.20.1",
    loader: { type: "forge", version: "47.4.0" },
    preservePaths: ["world/**", "server.properties"],
    files: Object.entries(contents).map(([path, value]) => ({
      type: "override",
      path,
      url: `data:application/octet-stream;base64,${Buffer.from(value).toString("base64")}`,
      size: Buffer.byteLength(value),
      hash: { algorithm: "sha256", value: sha256(value) }
    }))
  };
}

async function makeRoot() {
  const root = await mkdtemp(join(tmpdir(), "ruuudy-installer-"));
  await mkdir(join(root, SERVER_ID), { recursive: true });
  await writeFile(join(root, SERVER_ID, "unix_args.txt"), "@libraries/forge.txt");
  return root;
}

async function makeForgeSymlinkRuntimeRoot() {
  const root = await mkdtemp(join(tmpdir(), "ruuudy-installer-"));
  const serverRoot = join(root, SERVER_ID);
  await mkdir(join(serverRoot, "libraries/net/minecraftforge/forge/1.20.1-47.4.0"), {
    recursive: true
  });
  await writeFile(
    join(serverRoot, "libraries/net/minecraftforge/forge/1.20.1-47.4.0/unix_args.txt"),
    "@libraries/forge.txt"
  );
  await symlink(
    "libraries/net/minecraftforge/forge/1.20.1-47.4.0/unix_args.txt",
    join(serverRoot, "unix_args.txt")
  );
  return root;
}

describe("resolveServerPath", () => {
  it("rejects traversal and non-UUID server IDs", () => {
    expect(() => resolveServerPath("/minecraft/servers", "../etc")).toThrow(/uuid/i);
  });
});

describe("installFromManifest", () => {
  it("fresh wipe replaces the server and creates rollback data", async () => {
    const root = await makeRoot();
    await writeFile(join(root, SERVER_ID, "old.txt"), "old");

    const result = await installFromManifest({
      serversRoot: root,
      serverId: SERVER_ID,
      manifest: fixtureManifest({ "mods/new.jar": "new" }),
      mode: "wipe"
    });

    expect(await readFile(join(root, SERVER_ID, "mods/new.jar"), "utf8")).toBe("new");
    expect(await readFile(join(root, SERVER_ID, "unix_args.txt"), "utf8")).toBe(
      "@libraries/forge.txt"
    );
    await expect(readFile(join(root, SERVER_ID, "old.txt"), "utf8")).rejects.toThrow();
    expect(await readFile(join(result.rollbackPath, "old.txt"), "utf8")).toBe("old");
  });

  it.skipIf(process.platform === "win32")("preserves Forge unix_args symlinks during a wipe", async () => {
    const root = await makeForgeSymlinkRuntimeRoot();

    await installFromManifest({
      serversRoot: root,
      serverId: SERVER_ID,
      manifest: fixtureManifest({ "mods/new.jar": "new" }),
      mode: "wipe"
    });

    expect((await lstat(join(root, SERVER_ID, "unix_args.txt"))).isSymbolicLink()).toBe(true);
    expect(await readFile(join(root, SERVER_ID, "unix_args.txt"), "utf8")).toBe(
      "@libraries/forge.txt"
    );
  });

  it("preserve mode keeps matching server data and removes stale files", async () => {
    const root = await makeRoot();
    await mkdir(join(root, SERVER_ID, "world"), { recursive: true });
    await mkdir(join(root, SERVER_ID, "mods"), { recursive: true });
    await writeFile(join(root, SERVER_ID, "world/level.dat"), "world");
    await writeFile(join(root, SERVER_ID, "server.properties"), "motd=mine");
    await writeFile(join(root, SERVER_ID, "mods/stale.jar"), "stale");

    await installFromManifest({
      serversRoot: root,
      serverId: SERVER_ID,
      manifest: fixtureManifest({ "mods/new.jar": "new" }),
      mode: "preserve"
    });

    expect(await readFile(join(root, SERVER_ID, "world/level.dat"), "utf8")).toBe("world");
    expect(await readFile(join(root, SERVER_ID, "server.properties"), "utf8")).toBe("motd=mine");
    expect(await readFile(join(root, SERVER_ID, "mods/new.jar"), "utf8")).toBe("new");
    await expect(readFile(join(root, SERVER_ID, "mods/stale.jar"), "utf8")).rejects.toThrow();
  });

  it("does not mutate live files when a download hash fails", async () => {
    const root = await makeRoot();
    await writeFile(join(root, SERVER_ID, "old.txt"), "old");
    const manifest = fixtureManifest({ "mods/new.jar": "new" });
    manifest.files[0].hash.value = "0".repeat(64);

    await expect(
      installFromManifest({
        serversRoot: root,
        serverId: SERVER_ID,
        manifest,
        mode: "wipe"
      })
    ).rejects.toThrow(/hash/i);

    expect(await readFile(join(root, SERVER_ID, "old.txt"), "utf8")).toBe("old");
  });

  it("can restore the previous server from rollback", async () => {
    const root = await makeRoot();
    await writeFile(join(root, SERVER_ID, "old.txt"), "old");
    const result = await installFromManifest({
      serversRoot: root,
      serverId: SERVER_ID,
      manifest: fixtureManifest({ "mods/new.jar": "new" }),
      mode: "wipe"
    });

    await rollbackInstallation({
      serversRoot: root,
      serverId: SERVER_ID,
      rollbackPath: result.rollbackPath
    });

    expect(await readFile(join(root, SERVER_ID, "old.txt"), "utf8")).toBe("old");
    await expect(readFile(join(root, SERVER_ID, "mods/new.jar"), "utf8")).rejects.toThrow();
  });
});
