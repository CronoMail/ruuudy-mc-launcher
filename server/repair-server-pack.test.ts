import { mkdtemp, mkdir, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { describe, expect, it } from "vitest";

const execFileAsync = promisify(execFile);

describe("repair-server-pack", () => {
  it("keeps Prism in repaired server packs when ItemBorders depends on it", async () => {
    const dataDir = await mkdtemp(join(tmpdir(), "ruuudy-repair-pack-"));
    const packDir = join(dataDir, "packs", "BIOHAZARD");
    await mkdir(packDir, { recursive: true });
    await writeFile(
      join(packDir, "manifest.json"),
      `${JSON.stringify(
        {
          schemaVersion: 1,
          packId: "biohazard",
          packName: "BioHazard",
          version: "1.0.0",
          minecraftVersion: "1.20.1",
          loader: { type: "forge", version: "47.4.4" },
          files: [
            serverFile("ItemBorders", "ItemBorders-1.20.1-forge-1.2.2.jar", "a"),
            clientMarkedFile("Prism", "Prism-1.20.1-forge-1.0.5.jar", "b")
          ],
          overrides: []
        },
        null,
        2
      )}\n`,
      "utf8"
    );

    await execFileAsync(process.execPath, ["server/repair-server-pack.mjs", "BIOHAZARD"], {
      cwd: process.cwd(),
      env: { ...process.env, PACK_DATA_DIR: dataDir }
    });

    const repaired = JSON.parse(await readFile(join(packDir, "manifest.json"), "utf8"));

    expect(
      repaired.files.find((file: { filename: string }) => file.filename.startsWith("Prism"))?.side
    ).toBe("both");
  });
});

function serverFile(name: string, filename: string, hashPrefix: string) {
  return {
    type: "external",
    side: "both",
    required: true,
    name,
    filename,
    url: `https://cdn.example.test/${filename}`,
    sha256: hashPrefix.repeat(64),
    size: 1
  };
}

function clientMarkedFile(name: string, filename: string, hashPrefix: string) {
  return {
    ...serverFile(name, filename, hashPrefix),
    side: "client"
  };
}
