#!/usr/bin/env bash
set -euo pipefail
ulimit -c 0 || true

REAL_HOME="${HOME:?HOME must be set}"
ACCOUNT_HOME="${OPENCODE_CLAUDE_AUTH_ACCOUNT_HOME:-}"
PROFILE="${PROFILE:-work}"

APP_ARGS=()
while [[ $# -gt 0 ]]; do
  case "$1" in
    --profile)
      if [[ -n ${2:-} ]]; then
        PROFILE="$2"
        if [[ -z $ACCOUNT_HOME ]]; then
          ACCOUNT_HOME="$REAL_HOME/.cache/claude-multi/$PROFILE"
        fi
        shift 2
      else
        echo "Error: --profile requires an argument" >&2
        exit 1
      fi
      ;;
    --)
      shift
      APP_ARGS+=("$@")
      break
      ;;
    *)
      APP_ARGS+=("$1")
      shift
      ;;
  esac
done

if [[ -z $ACCOUNT_HOME ]]; then
  ACCOUNT_HOME="$REAL_HOME/.cache/claude-multi/$PROFILE"
fi

if [[ $ACCOUNT_HOME == "$REAL_HOME" ]]; then
  echo "Error: refusing to run claude2 against the real home" >&2
  exit 1
fi

CWD="$(pwd -P 2> /dev/null || pwd)"

mkdir -p "$ACCOUNT_HOME/.claude"
mkdir -p "$ACCOUNT_HOME/.config"
mkdir -p "$ACCOUNT_HOME/.local/share"
mkdir -p "$ACCOUNT_HOME/.cache"
mkdir -p "$ACCOUNT_HOME/.npm"

SANITIZED_PATH=""
IFS=':' read -ra PATH_DIRS <<< "${PATH:-}"
for dir in "${PATH_DIRS[@]}"; do
  case "$dir" in
    /nix/store/* | /run/current-system/sw/bin | /usr/* | /bin | /sbin | /run/wrappers/*)
      SANITIZED_PATH="${SANITIZED_PATH:+$SANITIZED_PATH:}$dir"
      ;;
  esac
done

CWD_BIND_ARGS=()
if [[ $CWD != "$REAL_HOME" ]]; then
  if [[ $CWD == "$REAL_HOME"/* ]]; then
    REL_CWD="${CWD#"$REAL_HOME"}"
    mkdir -p "$ACCOUNT_HOME$REL_CWD"
  fi
  CWD_BIND_ARGS=(--bind "$CWD" "$CWD")
fi

SSL_ARGS=()
for var in NIX_SSL_CERT_FILE SSL_CERT_FILE CURL_CA_BUNDLE; do
  if [[ -n ${!var:-} ]]; then
    SSL_ARGS+=(--setenv "$var" "${!var}")
  fi
done

exec @bwrap@ \
  --ro-bind / / \
  --dev /dev \
  --proc /proc \
  --tmpfs /tmp \
  --tmpfs /run/user \
  --bind "$ACCOUNT_HOME" "$REAL_HOME" \
  "${CWD_BIND_ARGS[@]}" \
  --clearenv \
  --setenv HOME "$REAL_HOME" \
  --setenv PATH "$SANITIZED_PATH" \
  --setenv TERM "${TERM:-xterm-256color}" \
  --setenv USER "$(id -un)" \
  --setenv LOGNAME "$(id -un)" \
  --setenv SHELL "${SHELL:-/bin/sh}" \
  --setenv LANG "${LANG:-en_US.UTF-8}" \
  --setenv TMPDIR /tmp \
  "${SSL_ARGS[@]}" \
  --die-with-parent \
  @claude@ "${APP_ARGS[@]}"
