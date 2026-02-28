import { describe, expect, it } from "vitest";

import { parentPath } from "@/modules/workspaces/workspaces-utils";

describe("workspaces utils", () => {
  it("returns parent paths correctly", () => {
    expect(parentPath("")).toBe("");
    expect(parentPath("config")).toBe("");
    expect(parentPath("config/app.yml")).toBe("config");
    expect(parentPath("/a/b/c/")).toBe("a/b");
  });
});
