import { createServer } from "node:http";
import { readFileSync } from "node:fs";
import { mkdir, readFile, stat, writeFile } from "node:fs/promises";
import { dirname, join, normalize } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const PORT = Number(process.env.PORT ?? 8787);
const ADMIN_TOKEN = readSecret("PACK_ADMIN_TOKEN", "PACK_ADMIN_TOKEN_FILE");
const DATA_DIR = process.env.PACK_DATA_DIR ?? join(__dirname, "data");
const CODE_PATTERN = /^[A-Z0-9_-]{2,32}$/;

const server = createServer(async (request, response) => {
  try {
    await route(request, response);
  } catch (error) {
    sendJson(response, error.statusCode ?? 500, {
      error: error instanceof Error ? error.message : "Internal server error"
    });
  }
});

server.listen(PORT, () => {
  console.log(`Ruuudy pack API listening on :${PORT}`);
});

async function route(request, response) {
  const url = new URL(request.url ?? "/", `http://${request.headers.host ?? "localhost"}`);

  if (request.method === "GET" && url.pathname === "/health") {
    sendJson(response, 200, { ok: true });
    return;
  }

  const publicMatch = url.pathname.match(/^\/api\/packs\/([A-Za-z0-9_-]+)$/);
  if (request.method === "GET" && publicMatch) {
    const code = normalizeCode(publicMatch[1]);
    const manifest = await readPackManifest(code);
    sendJson(response, 200, manifest);
    return;
  }

  const publicFileMatch = url.pathname.match(/^\/api\/packs\/([A-Za-z0-9_-]+)\/files\/(.+)$/);
  if (request.method === "GET" && publicFileMatch) {
    const code = normalizeCode(publicFileMatch[1]);
    const relativePath = decodeSafePath(publicFileMatch[2]);
    const filePath = packFilePath(code, relativePath);
    const file = await readFile(filePath);
    const details = await stat(filePath);
    response.writeHead(200, {
      "content-type": "application/octet-stream",
      "content-length": String(details.size),
      "cache-control": "public, max-age=31536000, immutable"
    });
    response.end(file);
    return;
  }

  const adminMatch = url.pathname.match(/^\/api\/admin\/packs\/([A-Za-z0-9_-]+)$/);
  if (request.method === "PUT" && adminMatch) {
    requireAdmin(request);
    const code = normalizeCode(adminMatch[1]);
    const body = await readJsonBody(request);
    validateManifest(body);
    await writePackManifest(code, body);
    sendJson(response, 200, {
      ok: true,
      code,
      manifestUrl: `/api/packs/${code}`,
      version: body.version
    });
    return;
  }

  const adminFileMatch = url.pathname.match(/^\/api\/admin\/packs\/([A-Za-z0-9_-]+)\/files\/(.+)$/);
  if (request.method === "PUT" && adminFileMatch) {
    requireAdmin(request);
    const code = normalizeCode(adminFileMatch[1]);
    const relativePath = decodeSafePath(adminFileMatch[2]);
    const body = await readRawBody(request, 100 * 1024 * 1024);
    const filePath = packFilePath(code, relativePath);
    await mkdir(dirname(filePath), { recursive: true });
    await writeFile(filePath, body);
    sendJson(response, 200, {
      ok: true,
      code,
      path: relativePath,
      size: body.length,
      url: `/api/packs/${code}/files/${encodeRelativePath(relativePath)}`
    });
    return;
  }

  sendJson(response, 404, { error: "Not found" });
}

function normalizeCode(code) {
  const normalized = code.trim().toUpperCase();
  if (!CODE_PATTERN.test(normalized)) {
    const error = new Error("Invalid pack code.");
    error.statusCode = 400;
    throw error;
  }
  return normalized;
}

function packManifestPath(code) {
  const path = normalize(join(DATA_DIR, "packs", code, "manifest.json"));
  const root = normalize(DATA_DIR);
  if (!path.startsWith(root)) {
    throw new Error("Unsafe pack path.");
  }
  return path;
}

function packFilePath(code, relativePath) {
  const path = normalize(join(DATA_DIR, "packs", code, "files", relativePath));
  const root = normalize(join(DATA_DIR, "packs", code, "files"));
  if (!path.startsWith(root)) {
    throw new Error("Unsafe pack file path.");
  }
  return path;
}

function decodeSafePath(value) {
  const decoded = decodeURIComponent(value).replaceAll("\\", "/");
  const parts = decoded.split("/").filter(Boolean);
  if (
    parts.length === 0 ||
    parts.some((part) => part === "." || part === ".." || part.includes("\0"))
  ) {
    throw httpError(400, "Invalid file path.");
  }
  return parts.join("/");
}

function encodeRelativePath(value) {
  return value.split("/").map((part) => encodeURIComponent(part)).join("/");
}

async function readPackManifest(code) {
  try {
    return JSON.parse(await readFile(packManifestPath(code), "utf8"));
  } catch (error) {
    const notFound = new Error(`Unknown pack code ${code}.`);
    notFound.statusCode = 404;
    throw notFound;
  }
}

async function writePackManifest(code, manifest) {
  const path = packManifestPath(code);
  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, `${JSON.stringify(manifest, null, 2)}\n`, "utf8");
}

async function readJsonBody(request) {
  const chunks = [];
  for await (const chunk of request) {
    chunks.push(chunk);
  }

  if (chunks.length === 0) {
    throw httpError(400, "Request body is required.");
  }

  return JSON.parse(Buffer.concat(chunks).toString("utf8"));
}

async function readRawBody(request, maxBytes) {
  const chunks = [];
  let size = 0;
  for await (const chunk of request) {
    size += chunk.length;
    if (size > maxBytes) {
      throw httpError(413, "Uploaded file is too large.");
    }
    chunks.push(chunk);
  }
  if (chunks.length === 0) {
    throw httpError(400, "Request body is required.");
  }
  return Buffer.concat(chunks);
}

function requireAdmin(request) {
  if (!ADMIN_TOKEN) {
    throw httpError(503, "PACK_ADMIN_TOKEN is not configured.");
  }

  const auth = request.headers.authorization ?? "";
  if (auth !== `Bearer ${ADMIN_TOKEN}`) {
    throw httpError(401, "Invalid admin token.");
  }
}

function validateManifest(manifest) {
  if (manifest?.schemaVersion !== 1) {
    throw httpError(400, "Unsupported or missing manifest schemaVersion.");
  }

  for (const field of ["packId", "packName", "version", "minecraftVersion"]) {
    if (typeof manifest[field] !== "string" || manifest[field].trim() === "") {
      throw httpError(400, `Manifest field ${field} is required.`);
    }
  }

  if (manifest.loader?.type !== "fabric" || typeof manifest.loader?.version !== "string") {
    throw httpError(400, "Only Fabric manifests are supported.");
  }

  if (!Array.isArray(manifest.files) || !Array.isArray(manifest.overrides)) {
    throw httpError(400, "Manifest files and overrides must be arrays.");
  }
}

function sendJson(response, statusCode, payload) {
  response.writeHead(statusCode, {
    "content-type": "application/json; charset=utf-8",
    "cache-control": statusCode === 200 ? "no-store" : "no-cache"
  });
  response.end(`${JSON.stringify(payload)}\n`);
}

function httpError(statusCode, message) {
  const error = new Error(message);
  error.statusCode = statusCode;
  return error;
}

function readSecret(valueName, fileName) {
  const directValue = process.env[valueName];
  if (directValue) {
    return directValue;
  }

  const filePath = process.env[fileName];
  if (!filePath) {
    return "";
  }

  return readFileSync(filePath, "utf8").trim();
}
