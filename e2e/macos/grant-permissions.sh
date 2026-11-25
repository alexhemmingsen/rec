#!/usr/bin/env bash
# CI only: grant Screen Recording to Rec.app and Accessibility/event posting to
# the processes that drive the end-to-end test. GitHub's macOS runners have
# System Integrity Protection disabled, which makes the TCC databases
# writable. Never run this on a real machine.
set -uo pipefail

SYSTEM_DB="/Library/Application Support/com.apple.TCC/TCC.db"
USER_DB="$HOME/Library/Application Support/com.apple.TCC/TCC.db"
NOW=$(date +%s)

csrutil status || true
echo "access table schema:"
sudo sqlite3 "$SYSTEM_DB" ".schema access" || true

grant() { # service client client_type(0 = bundle id, 1 = path)
  for db in "$SYSTEM_DB" "$USER_DB"; do
    [[ -f "$db" ]] || continue
    sudo sqlite3 "$db" "INSERT OR REPLACE INTO access (service, client, client_type, auth_value, auth_reason, auth_version, indirect_object_identifier, flags, last_modified) VALUES ('$1', '$2', $3, 2, 4, 1, 'UNUSED', 0, $NOW);" \
      && echo "granted $1 to $2 in $(basename "$(dirname "$db")")/$(basename "$db")" \
      || echo "could not grant $1 to $2 in $db"
  done
}

# Rec itself records the screen.
grant kTCCServiceScreenCapture com.alexhem.rec 0

# The driver and everything above it (the test runner's process chain) post
# events and may be the "responsible" process for the live capture test.
clients=("$@")
pid=$$
while [[ "$pid" -gt 1 ]]; do
  exe=$(ps -o comm= -p "$pid" 2>/dev/null | sed 's/^ *//')
  [[ "$exe" == /* ]] && clients+=("$exe")
  pid=$(ps -o ppid= -p "$pid" 2>/dev/null | tr -d ' ')
  [[ -z "$pid" ]] && break
done
for c in /bin/bash /bin/zsh /bin/sh /usr/local/opt/runner/provisioner/provisioner /opt/homebrew/opt/runner/provisioner/provisioner; do
  clients+=("$c")
done
for c in "${clients[@]}"; do
  for svc in kTCCServiceAccessibility kTCCServicePostEvent kTCCServiceScreenCapture; do
    grant "$svc" "$c" 1
  done
done

# Make tccd pick up the change.
sudo killall tccd 2>/dev/null || true
sleep 2
