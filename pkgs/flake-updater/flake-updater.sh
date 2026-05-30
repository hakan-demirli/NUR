#!/usr/bin/env bash
set -uo pipefail

JOBS=$(nproc)

usage() {
  echo "Usage: flake-updater [-j [N]]"
  echo "  -j [N]  Number of parallel jobs (default: nproc = $(nproc))."
  echo "          If N is omitted, defaults to nproc."
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    -j)
      if [[ ${2:-} =~ ^[0-9]+$ ]]; then
        JOBS=$2
        shift 2
      else
        JOBS=$(nproc)
        shift
      fi
      ;;
    -j*)
      arg=${1#-j}
      if [[ $arg =~ ^[0-9]+$ ]]; then
        JOBS=$arg
        shift
      else
        echo "[ERROR] Invalid value for -j: '$arg'"
        usage
        exit 1
      fi
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      echo "[ERROR] Unknown argument: '$1'"
      usage
      exit 1
      ;;
  esac
done

if [[ $JOBS -lt 1 ]]; then
  echo "[ERROR] -j must be >= 1."
  exit 1
fi

echo "[DEBUG] Checking upstream for latest NixOS version..."
TARGET_VERSION=$(git ls-remote --sort=-v:refname https://github.com/NixOS/nixpkgs 'nixos-??.??' | awk '{ sub("^.*/","",$2); print $2; exit}')

if [[ -z $TARGET_VERSION ]]; then
  echo "[ERROR]  Failed to fetch latest version."
  exit 1
fi

echo "[DEBUG] Target Version: $TARGET_VERSION"
echo "[DEBUG] Running with up to $JOBS parallel job(s)."

LOCKFILE=$(mktemp)
export TARGET_VERSION LOCKFILE

cleanup() { rm -f "$LOCKFILE"; }
trap cleanup EXIT

process_flake() {
  local flake_file=$1
  local dir
  dir=$(dirname "$flake_file")

  local out=""
  emit() { out+="$1"$'\n'; }

  local CURRENT_VERSION
  CURRENT_VERSION=$(grep -oE "nixos-[0-9]{2}\.[0-9]{2}" "$flake_file" | head -n1)

  if [[ -z $CURRENT_VERSION ]]; then
    emit "[WARN]  Could not determine version for $dir. Skipping."
    flush "$out"
    return
  fi

  emit "------------------------------------------------"
  emit "[DEBUG] Processing: $dir ($CURRENT_VERSION)"

  local VERSION_CHANGED=false

  if [[ $CURRENT_VERSION != "$TARGET_VERSION" ]]; then
    emit "[DEBUG] Version bump required: $CURRENT_VERSION -> $TARGET_VERSION"

    if [[ $OSTYPE == "darwin"* ]]; then
      sed -i '' "s/$CURRENT_VERSION/$TARGET_VERSION/g" "$flake_file"
    else
      sed -i "s/$CURRENT_VERSION/$TARGET_VERSION/g" "$flake_file"
    fi
    VERSION_CHANGED=true
  fi

  emit "[DEBUG] Running 'nix flake update'..."
  if (cd "$dir" && nix flake update &> /dev/null); then

    local LOCK_CHANGED=false
    if ! git diff --quiet "$dir/flake.lock"; then
      LOCK_CHANGED=true
    fi

    if [[ $VERSION_CHANGED == "true" ]] || [[ $LOCK_CHANGED == "true" ]]; then
      if [[ $VERSION_CHANGED == "true" ]]; then
        emit "[INFO]  $dir: Upgraded to $TARGET_VERSION"
      elif [[ $LOCK_CHANGED == "true" ]]; then
        emit "[INFO]  $dir: Remained on $TARGET_VERSION, but inputs updated (Backports/Fixes)"
      fi

      emit "[DEBUG] Verifying build..."
      if (cd "$dir" && nix flake check &> /dev/null); then
        emit "[SUCCESS] $dir is healthy."
      else
        emit "[ERROR]   $dir: Check Failed (Build broken after update)"
      fi
    else
      emit "[INFO]  $dir: Already up to date (No changes in version or lockfile)."
    fi
  else
    emit "[ERROR]   $dir: 'nix flake update' failed."
  fi

  flush "$out"
}

flush() {
  (
    flock 9
    printf "%s" "$1"
  ) 9> "$LOCKFILE"
}

export -f process_flake flush

find . -type f -name "flake.nix" -not -path "*/.git/*" -not -path "*/_deprecated/*" -print0 \
  | xargs -0 -r -P "$JOBS" -I {} bash -c 'process_flake "$@"' _ {}

echo "Done."
