import { describe, expect, it } from "vitest";
import { formatUpdateStatus } from "./updater";

describe("formatUpdateStatus", () => {
  it("shows the available version when an update exists", () => {
    expect(formatUpdateStatus({ state: "available", version: "0.1.1" })).toBe("Update 0.1.1 available");
  });

  it("keeps the idle button label short", () => {
    expect(formatUpdateStatus({ state: "idle" })).toBe("Check for Updates");
  });
});
