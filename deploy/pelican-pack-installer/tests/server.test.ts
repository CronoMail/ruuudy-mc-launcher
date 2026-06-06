import { createHash } from "node:crypto";
import { mkdtemp, mkdir } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { createAgentServer } from "../src/server.mjs";

const SERVER_ID = "34e80249-582d-47fe-9680-8dd3a64411cc";
const servers: Array<ReturnType<typeof createAgentServer>> = [];

function manifest() {
  const value = "server-file";
  return {
    schemaVersion: 1,
    code: "TEST",
    packId: "test",
    packName: "Test",
    version: "1",
    minecraftVersion: "1.20.1",
    loader: { type: "forge", version: "47.4.0" },
    preservePaths: [],
    files: [
      {
        type: "override",
        path: "mods/test.jar",
        url: `data:application/octet-stream;base64,${Buffer.from(value).toString("base64")}`,
        size: value.length,
        hash: {
          algorithm: "sha256",
          value: createHash("sha256").update(value).digest("hex")
        }
      }
    ]
  };
}

async function startAgent() {
  const root = await mkdtemp(join(tmpdir(), "ruuudy-agent-"));
  await mkdir(join(root, SERVER_ID));
  const agent = createAgentServer({
    token: "secret",
    serversRoot: root,
    resolveManifest: async () => manifest()
  });
  servers.push(agent);
  await new Promise<void>((resolve) => agent.listen(0, "127.0.0.1", resolve));
  const address = agent.address();
  if (!address || typeof address === "string") throw new Error("No test address");
  return `http://127.0.0.1:${address.port}`;
}

afterEach(async () => {
  await Promise.all(
    servers.splice(0).map(
      (server) =>
        new Promise<void>((resolve) => {
          server.close(() => resolve());
        })
    )
  );
});

describe("installer agent", () => {
  it("requires bearer authentication", async () => {
    const base = await startAgent();
    const response = await fetch(`${base}/v1/installations`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ serverId: SERVER_ID, code: "TEST", mode: "wipe" })
    });
    expect(response.status).toBe(401);
  });

  it("runs an install and exposes structured progress", async () => {
    const base = await startAgent();
    const response = await fetch(`${base}/v1/installations`, {
      method: "POST",
      headers: {
        authorization: "Bearer secret",
        "content-type": "application/json"
      },
      body: JSON.stringify({ serverId: SERVER_ID, code: "TEST", mode: "wipe" })
    });
    expect(response.status).toBe(202);
    const created = await response.json();

    let job;
    for (let attempt = 0; attempt < 30; attempt += 1) {
      const status = await fetch(`${base}/v1/installations/${created.jobId}`, {
        headers: { authorization: "Bearer secret" }
      });
      job = await status.json();
      if (job.status === "completed" || job.status === "failed") break;
      await new Promise((resolve) => setTimeout(resolve, 10));
    }

    expect(job.status).toBe("completed");
    expect(job.progress.phase).toBe("completed");
    expect(job.result.rollbackPath).toContain(".ruuudy-pack-installer");
  });
});

