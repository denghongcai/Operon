# Operon portable-pty 0.9.0 patch

Source: crates.io portable-pty 0.9.0, upstream Wez Furlong / wezterm,
MIT license as declared in the unmodified package manifest.

Only behavioral change: Windows CommandBuilder::environment_block emits
two UTF-16 NULs for an empty environment. Nonempty blocks remain unchanged.
Two Windows unit regressions cover the empty and nonempty representations.
Operon native PTY smoke clears its environment and the actual public SDK gate
uses preserve_env=false with an empty allowlist. Do not restore ambient secrets
or PATH as a workaround for malformed CreateProcessW input.

The manifest has an isolated workspace declaration and its own committed test
lockfile so upstream regression tests run without becoming Operon workspace
members. Production uses Operon's root lockfile and the crates.io path patch.
