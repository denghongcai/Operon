export function nativeTarget(platform: string, arch: string, musl = false): string {
  const cpu = arch === "x64" ? "x86_64" : arch === "arm64" ? "aarch64" : undefined;
  if (platform === "linux" && cpu) return `${cpu}-unknown-linux-${musl ? "musl" : "gnu"}`;
  if (platform === "darwin" && cpu) return `${cpu}-apple-darwin`;
  if (platform === "win32" && arch === "x64") return "x86_64-pc-windows-msvc";
  throw new Error(`no bundled iroh binding for ${platform}/${arch}; use a supported platform or an explicit OPERON_SDK_NATIVE_PATH`);
}
