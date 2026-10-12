import { describe, expect, it } from "vitest";
import { nativeTarget } from "./native-target";

describe("bundled native platform selection", () => {
  it.each([
    ["linux", "x64", false, "x86_64-unknown-linux-gnu"],
    ["linux", "arm64", false, "aarch64-unknown-linux-gnu"],
    ["linux", "x64", true, "x86_64-unknown-linux-musl"],
    ["linux", "arm64", true, "aarch64-unknown-linux-musl"],
    ["darwin", "x64", false, "x86_64-apple-darwin"],
    ["darwin", "arm64", false, "aarch64-apple-darwin"],
    ["win32", "x64", false, "x86_64-pc-windows-msvc"],
  ])("maps %s/%s (musl=%s)", (os, cpu, musl, target) => {
    expect(nativeTarget(os, cpu, musl)).toBe(target);
  });
  it.each([["linux", "arm"], ["win32", "arm64"], ["freebsd", "x64"]])("rejects unsupported %s/%s", (os, cpu) => {
    expect(() => nativeTarget(os, cpu)).toThrow("no bundled iroh binding");
  });
});
