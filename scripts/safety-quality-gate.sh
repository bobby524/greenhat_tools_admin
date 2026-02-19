#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

step() {
  echo
  echo "==> $1"
}

step "Runtime panic guard"
./scripts/check-runtime-panics.sh

step "Formatting"
cargo fmt --all -- --check

step "Compile checks"
cargo check --workspace --all-targets

step "Tests"
cargo test -p gateway -p mcp-spike

step "Key security smoke validations"
cargo test -p gateway --test headers --test csrf --test rbac --test egress --test middleware

echo
echo "[OK] safety quality gate passed"
