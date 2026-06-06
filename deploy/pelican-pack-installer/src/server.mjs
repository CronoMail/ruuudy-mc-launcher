import { randomUUID } from "node:crypto";
import { createServer } from "node:http";
import { pathToFileURL } from "node:url";
import { installFromManifest, rollbackInstallation } from "./installer.mjs";

export function createAgentServer({
  token,
  serversRoot,
  packApiBase = "",
  resolveManifest = null
}) {
  if (!token) throw new Error("PACK_INSTALLER_TOKEN is required.");
  if (!serversRoot) throw new Error("PELICAN_SERVERS_ROOT is required.");

  const jobs = new Map();
  const activeServers = new Set();
  const manifestResolver =
    resolveManifest ??
    (async (code) => {
      const response = await fetch(
        `${packApiBase.replace(/\/+$/, "")}/api/packs/${encodeURIComponent(code)}/server-manifest`
      );
      if (!response.ok) {
        const body = await response.text();
        throw new Error(`Pack API rejected ${code}: HTTP ${response.status} ${body}`.trim());
      }
      return response.json();
    });

  return createServer(async (request, response) => {
    try {
      const url = new URL(request.url ?? "/", `http://${request.headers.host ?? "localhost"}`);
      if (request.method === "GET" && url.pathname === "/health") {
        sendJson(response, 200, { ok: true });
        return;
      }
      requireBearer(request, token);

      if (request.method === "POST" && url.pathname === "/v1/installations") {
        const body = await readJson(request);
        validateInstallRequest(body);
        if (activeServers.has(body.serverId)) {
          throw httpError(409, "An installation is already active for this server.");
        }

        const job = {
          jobId: randomUUID(),
          serverId: body.serverId,
          code: body.code.trim().toUpperCase(),
          mode: body.mode,
          status: "queued",
          progress: { phase: "queued", completed: 0, total: 0 },
          createdAt: new Date().toISOString(),
          updatedAt: new Date().toISOString(),
          result: null,
          error: null
        };
        jobs.set(job.jobId, job);
        activeServers.add(job.serverId);
        runInstall(job, body, {
          serversRoot,
          manifestResolver,
          activeServers
        });
        sendJson(response, 202, publicJob(job));
        return;
      }

      const jobMatch = url.pathname.match(/^\/v1\/installations\/([0-9a-f-]+)$/i);
      if (request.method === "GET" && jobMatch) {
        const job = requireJob(jobs, jobMatch[1]);
        sendJson(response, 200, publicJob(job));
        return;
      }

      const rollbackMatch = url.pathname.match(
        /^\/v1\/installations\/([0-9a-f-]+)\/rollback$/i
      );
      if (request.method === "POST" && rollbackMatch) {
        const job = requireJob(jobs, rollbackMatch[1]);
        if (job.status !== "completed" || !job.result?.rollbackPath) {
          throw httpError(409, "Only completed installations with rollback data can be restored.");
        }
        const result = await rollbackInstallation({
          serversRoot,
          serverId: job.serverId,
          rollbackPath: job.result.rollbackPath
        });
        sendJson(response, 200, { ok: true, result });
        return;
      }

      sendJson(response, 404, { error: "Not found" });
    } catch (error) {
      sendJson(response, error.statusCode ?? 500, {
        error: error instanceof Error ? error.message : "Internal server error"
      });
    }
  });
}

async function runInstall(job, body, { serversRoot, manifestResolver, activeServers }) {
  try {
    updateJob(job, { status: "fetching", progress: { phase: "fetching", completed: 0, total: 0 } });
    const manifest = await manifestResolver(job.code);
    updateJob(job, { status: "installing" });
    const result = await installFromManifest({
      serversRoot,
      serverId: job.serverId,
      manifest,
      mode: job.mode,
      preservePaths: Array.isArray(body.preservePaths) ? body.preservePaths : [],
      onProgress: (progress) => updateJob(job, { progress })
    });
    updateJob(job, { status: "completed", result });
  } catch (error) {
    updateJob(job, {
      status: "failed",
      error: error instanceof Error ? error.message : "Installation failed"
    });
  } finally {
    activeServers.delete(job.serverId);
  }
}

function updateJob(job, fields) {
  Object.assign(job, fields, { updatedAt: new Date().toISOString() });
}

function publicJob(job) {
  return {
    jobId: job.jobId,
    serverId: job.serverId,
    code: job.code,
    mode: job.mode,
    status: job.status,
    progress: job.progress,
    createdAt: job.createdAt,
    updatedAt: job.updatedAt,
    result: job.result,
    error: job.error
  };
}

function requireJob(jobs, jobId) {
  const job = jobs.get(jobId);
  if (!job) throw httpError(404, "Unknown installation job.");
  return job;
}

function validateInstallRequest(body) {
  if (typeof body?.serverId !== "string" || typeof body?.code !== "string") {
    throw httpError(400, "serverId and code are required.");
  }
  if (!["preserve", "wipe"].includes(body.mode)) {
    throw httpError(400, "mode must be preserve or wipe.");
  }
  if (
    body.preservePaths !== undefined &&
    (!Array.isArray(body.preservePaths) ||
      body.preservePaths.some((value) => typeof value !== "string"))
  ) {
    throw httpError(400, "preservePaths must be an array of strings.");
  }
}

function requireBearer(request, token) {
  if (request.headers.authorization !== `Bearer ${token}`) {
    throw httpError(401, "Unauthorized.");
  }
}

async function readJson(request) {
  const chunks = [];
  for await (const chunk of request) chunks.push(chunk);
  if (chunks.length === 0) throw httpError(400, "Request body is required.");
  try {
    return JSON.parse(Buffer.concat(chunks).toString("utf8"));
  } catch {
    throw httpError(400, "Request body must be valid JSON.");
  }
}

function sendJson(response, status, body) {
  response.writeHead(status, { "content-type": "application/json; charset=utf-8" });
  response.end(`${JSON.stringify(body)}\n`);
}

function httpError(statusCode, message) {
  const error = new Error(message);
  error.statusCode = statusCode;
  return error;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const server = createAgentServer({
    token: process.env.PACK_INSTALLER_TOKEN ?? "",
    serversRoot: process.env.PELICAN_SERVERS_ROOT ?? "/minecraft/servers",
    packApiBase: process.env.PACK_API_BASE ?? "https://launcher.ruuudy.in"
  });
  const port = Number(process.env.PORT ?? 8790);
  server.listen(port, "0.0.0.0", () => {
    console.log(`Ruuudy Pelican pack installer listening on :${port}`);
  });
}

