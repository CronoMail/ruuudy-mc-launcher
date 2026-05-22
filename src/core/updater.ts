export type UpdateStatus =
  | { state: "idle" }
  | { state: "checking" }
  | { state: "available"; version: string }
  | { state: "downloading" }
  | { state: "current" }
  | { state: "error"; message: string };

export function formatUpdateStatus(status: UpdateStatus): string {
  switch (status.state) {
    case "checking":
      return "Checking...";
    case "available":
      return `Update ${status.version} available`;
    case "downloading":
      return "Installing update...";
    case "current":
      return "Launcher is up to date";
    case "error":
      return status.message;
    case "idle":
    default:
      return "Check for Updates";
  }
}
