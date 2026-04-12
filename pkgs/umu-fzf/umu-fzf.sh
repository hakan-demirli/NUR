#!/usr/bin/env bash
set -euo pipefail

readonly CACHE_DIR="${XDG_CACHE_HOME:-$HOME/.cache}/umu-fzf"
readonly CACHE_FILE="$CACHE_DIR/games.tsv"
readonly DB_URL="https://raw.githubusercontent.com/jsnli/steamappidlist/master/data/games_appid.json"

die() {
  printf "error: %s\n" "$*" >&2
  exit 1
}

check_deps() {
  local missing=()
  for cmd in curl jq fzf wl-copy; do
    command -v "$cmd" > /dev/null 2>&1 || missing+=("$cmd")
  done
  [[ ${#missing[@]} -eq 0 ]] || die "missing dependencies: ${missing[*]}"
}

pull() {
  mkdir -p "$CACHE_DIR"
  printf "Downloading Steam game list...\n" >&2
  curl -sL "$DB_URL" \
    | jq -r '.[] | "\(.appid)\t\(.name)"' \
    | sort -t$'\t' -k2f \
      > "$CACHE_FILE.tmp"
  mv "$CACHE_FILE.tmp" "$CACHE_FILE"
  printf "Cached %d games to %s\n" "$(wc -l < "$CACHE_FILE")" "$CACHE_FILE" >&2
}

ensure_cache() {
  [[ -f $CACHE_FILE ]] || die "no cache found. Run: $(basename "$0") pull"
}

select_game() {
  ensure_cache

  local selected
  selected=$(fzf \
    --delimiter=$'\t' \
    --with-nth=2 \
    --preview='echo "GAMEID=umu-{1}"' \
    --preview-window=up:1 \
    --prompt="Game> " \
    < "$CACHE_FILE") || exit 0

  local appid name result
  appid=$(printf "%s" "$selected" | cut -f1)
  name=$(printf "%s" "$selected" | cut -f2)
  result="GAMEID=umu-${appid}"

  printf "%s" "$result" | wl-copy
  printf "%s  # %s\n" "$result" "$name"
}

usage() {
  cat << EOF
Usage: $(basename "$0") [command]

Fuzzy-find a Steam game and copy its umu-run GAMEID to clipboard.

Commands:
  (none)    Open fzf picker, copy GAMEID to clipboard
  pull      Download/update the Steam game database
  help      Show this help

Dependencies: curl, jq, fzf, wl-copy
EOF
  exit 0
}

main() {
  check_deps

  case "${1:-}" in
    pull) pull ;;
    help | -h | --help) usage ;;
    "") select_game ;;
    *) die "unknown command: $1. Run: $(basename "$0") help" ;;
  esac
}

main "$@"
