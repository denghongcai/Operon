#!/usr/bin/env bash

require_file() {
  local path="$1"
  test -f "$path" || {
    echo "missing required file: $path" >&2
    exit 1
  }
}

require_pattern() {
  local pattern="$1"
  local path="$2"
  rg -n "$pattern" "$path" >/dev/null || {
    echo "missing required pattern in $path: $pattern" >&2
    exit 1
  }
}

reject_pattern() {
  local pattern="$1"
  local path="$2"
  if rg -n "$pattern" "$path"; then
    echo "unexpected pattern in $path: $pattern" >&2
    exit 1
  fi
}
