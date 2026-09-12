#!/usr/bin/env bash
# Usage: sudo <app> > import_connections.sh

set -euo pipefail

if [ "$(id -u)" -ne 0 ]; then
  echo "Error: This script must be run as root to access NetworkManager secrets." >&2
  exit 1
fi

cat << 'IMPORT_HEADER'
#!/usr/bin/env bash
# Generated import script for NetworkManager connections
# strips hardware-specific bindings (MAC, interface) for portability

set -euo pipefail

if [ "$(id -u)" -ne 0 ]; then
  echo 'Please run as root' >&2
  exit 1
fi

target_dir=/etc/NetworkManager/system-connections
install -d -m 700 "$target_dir"

import() {
  local uuid=$1 name=$2 target="$target_dir/$1.nmconnection"
  echo "Importing $name ($uuid)..."
  grep -rlx "uuid=$uuid" "$target_dir" | xargs -r rm -f || true
  cat > "$target"
  chmod 600 "$target"
  chown root:root "$target"
}

IMPORT_HEADER

while IFS=: read -r uuid type file; do
  if [ "$type" = loopback ]; then
    continue
  fi

  if [ ! -r "$file" ]; then
    echo "Error: keyfile for connection $uuid is unreadable: $file" >&2
    exit 1
  fi

  name=$(nmcli --get-values connection.id connection show "$uuid")

  printf "import %q %q << 'NMCONNECTION'\n" "$uuid" "$name"
  sed -E '/^(mac-address=|interface-name=|permissions=)/d' "$file"
  echo NMCONNECTION
  echo
done < <(nmcli --get-values UUID,TYPE,FILENAME connection show)

cat << 'IMPORT_FOOTER'
echo 'Reloading NetworkManager connections...'
nmcli connection reload
echo 'Done. Connections imported and reloaded.'
IMPORT_FOOTER
