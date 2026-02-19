#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
LIB="$ROOT/gateway/src/lib.rs"
MAIN="$ROOT/gateway/src/main.rs"

# Scan only non-test region of lib.rs
cut_line=$(awk '/^#\[cfg\(test\)\]/{print NR; exit}' "$LIB")
if [[ -z "${cut_line:-}" ]]; then
  cut_line=$(wc -l < "$LIB")
fi

violations=$(sed -n "1,${cut_line}p" "$LIB" | rg -n "\b(unwrap\(|expect\(|panic!\()" || true)
# allow startup-fail panic for invalid RBAC policy loading (not request-path runtime)
violations=$(echo "$violations" | rg -v "failed to load policy from" || true)
main_violations=$(rg -n "\b(unwrap\(|expect\(|panic!\()" "$MAIN" || true)

if [[ -n "$violations" || -n "$main_violations" ]]; then
  echo "[FAIL] runtime panic-prone calls found"
  [[ -n "$violations" ]] && echo "-- lib.rs --" && echo "$violations"
  [[ -n "$main_violations" ]] && echo "-- main.rs --" && echo "$main_violations"
  exit 1
fi

echo "[OK] no unwrap/expect/panic in runtime request paths (lib non-test + main)"
