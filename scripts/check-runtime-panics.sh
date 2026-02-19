#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
LIB="$ROOT/gateway/src/lib.rs"
MAIN="$ROOT/gateway/src/main.rs"
ALLOWLIST="$ROOT/scripts/runtime-panic-allowlist.txt"

if [[ ! -f "$ALLOWLIST" ]]; then
  echo "[FAIL] missing allowlist file: $ALLOWLIST"
  exit 1
fi

# Scan only non-test region of lib.rs.
cut_line=$(awk '/^#\[cfg\(test\)\]/{print NR; exit}' "$LIB")
if [[ -z "${cut_line:-}" ]]; then
  cut_line=$(wc -l < "$LIB")
fi

lib_hits=$(sed -n "1,${cut_line}p" "$LIB" | rg -n "\b(unwrap\(|expect\(|panic!\()" | sed "s#^#$LIB:#" || true)
main_hits=$(rg -nH "\b(unwrap\(|expect\(|panic!\()" "$MAIN" || true)

combined=$(printf "%s\n%s\n" "$lib_hits" "$main_hits" | sed '/^$/d')
remaining="$combined"

while IFS= read -r rule; do
  [[ -z "$rule" || "$rule" =~ ^[[:space:]]*# ]] && continue

  if [[ "$rule" != *"::"* ]]; then
    echo "[FAIL] malformed allowlist rule (expected file_substring::code_substring): $rule"
    exit 1
  fi

  file_sub="${rule%%::*}"
  code_sub="${rule#*::}"

  remaining=$(printf "%s\n" "$remaining" | awk -v file_sub="$file_sub" -v code_sub="$code_sub" '
    index($0, file_sub) && index($0, code_sub) { next }
    { print }
  ')
done < "$ALLOWLIST"

remaining=$(printf "%s\n" "$remaining" | sed '/^$/d')

if [[ -n "$remaining" ]]; then
  echo "[FAIL] runtime panic-prone calls found outside allowlist"
  echo "Allowlist: $ALLOWLIST"
  echo "$remaining"
  exit 1
fi

echo "[OK] runtime panic guard passed (tests excluded; startup-only allowlist applied)"