export type UpdateStatus =
  | { state: "idle" }
  | { state: "checking" }
  | { state: "available"; version: string }
  | { state: "downloading"; version?: string; downloadedBytes?: number; totalBytes?: number | null }
  | { state: "installing"; version?: string }
  | { state: "restarting"; version?: string }
  | { state: "current" }
  | { state: "error"; message: string };

export function formatUpdateStatus(status: UpdateStatus): string {
  switch (status.state) {
    case "checking":
      return "Checking...";
    case "available":
      return `Update ${status.version} available`;
    case "downloading":
      if (status.totalBytes && status.downloadedBytes !== undefined) {
        return `Downloading ${Math.min(100, Math.round((status.downloadedBytes / status.totalBytes) * 100))}%`;
      }
      return "Downloading update...";
    case "installing":
      return "Installing update...";
    case "restarting":
      return "Restarting launcher...";
    case "current":
      return "Launcher is up to date";
    case "error":
      return status.message;
    case "idle":
    default:
      return "Check for Updates";
  }
}
