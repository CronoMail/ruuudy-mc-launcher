import { resolve, sep } from "node:path";

const UUID_PATTERN =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;

export function resolveServerPath(serversRoot, serverId) {
  if (!UUID_PATTERN.test(serverId)) {
    throw new Error("Server ID must be a UUID.");
  }
  return resolveInside(serversRoot, serverId);
}

export function resolveInside(root, relativePath) {
  const rootPath = resolve(root);
  const target = resolve(rootPath, relativePath);
  if (target !== rootPath && !target.startsWith(`${rootPath}${sep}`)) {
    throw new Error("Resolved path escapes its configured root.");
  }
  return target;
}

export function safeRelativePath(value) {
  if (typeof value !== "string") {
    throw new Error("File path must be a string.");
  }
  const normalized = value.replaceAll("\\", "/");
  const parts = normalized.split("/").filter(Boolean);
  if (
    parts.length === 0 ||
    normalized.startsWith("/") ||
    parts.some((part) => part === "." || part === ".." || part.includes("\0"))
  ) {
    throw new Error(`Unsafe file path '${value}'.`);
  }
  return parts.join("/");
}

