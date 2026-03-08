#!/usr/bin/env bash

set -uo pipefail
TOP_N=5
COLOR=1

while [[ $# -gt 0 ]]; do
  case "$1" in
    --top)
      TOP_N="$2"
      shift 2
      ;;
    --no-color)
      COLOR=0
      shift
      ;;
    -h | --help)
      echo "Usage: nix-audit [--top N] [--no-color]"
      echo ""
      echo "Analyze Nix store disk usage and identify what's pinning space."
      echo ""
      echo "Options:"
      echo "  --top N       Show top N store paths per root (default: 5)"
      echo "  --no-color    Disable colored output"
      echo "  -h, --help    Show this help"
      exit 0
      ;;
    *)
      echo "Unknown option: $1"
      exit 1
      ;;
  esac
done

if [[ $COLOR -eq 1 ]] && [[ -t 1 ]]; then
  BOLD='\033[1m'
  DIM='\033[2m'
  GREEN='\033[0;32m'
  YELLOW='\033[0;33m'
  BLUE='\033[0;34m'
  CYAN='\033[0;36m'
  RESET='\033[0m'
else
  BOLD='' DIM='' GREEN='' YELLOW='' BLUE='' CYAN='' RESET=''
fi

hr() {
  printf "${DIM}%0.s─${RESET}" $(seq 1 78)
  echo
}
header() {
  echo
  hr
  printf "${BOLD}${CYAN}  %s${RESET}\n" "$1"
  hr
}
subhdr() { printf "\n${BOLD}${BLUE}  ▸ %s${RESET}\n" "$1"; }
warn() { printf "  ${YELLOW}⚠ %s${RESET}\n" "$1"; }
info() { printf "  ${DIM}%s${RESET}\n" "$1"; }
good() { printf "  ${GREEN}✓ %s${RESET}\n" "$1"; }

human_size() {
  local kb=$1
  if ((kb >= 1048576)); then
    printf "%s GB" "$(awk "BEGIN {printf \"%.1f\", $kb / 1048576}")"
  elif ((kb >= 1024)); then
    printf "%s MB" "$(awk "BEGIN {printf \"%.0f\", $kb / 1024}")"
  else
    printf "%d KB" "$kb"
  fi
}

paths_size_kb() {
  local input
  input=$(cat)
  if [[ -z $input ]]; then
    echo 0
    return
  fi
  local total
  total=$(xargs du -sc 2> /dev/null <<< "$input" | tail -1 | awk '{print $1}')
  echo "${total:-0}"
}

reqs() { nix-store -q --requisites "$1" 2> /dev/null | sort -u; }

TMPDIR_AUDIT=$(mktemp -d)
trap 'rm -rf "$TMPDIR_AUDIT"' EXIT

header "Nix Store Overview"

if mountpoint -q /nix 2> /dev/null; then
  STORE_SIZE_KB=$(df -k /nix | awk 'NR==2 {print $3}')
else
  STORE_SIZE_KB=$(du -sk /nix/store 2> /dev/null | awk '{print $1}')
fi
STORE_PATH_COUNT=$(find /nix/store -maxdepth 1 -mindepth 1 2> /dev/null | wc -l)
DEAD_COUNT=$(nix-store --gc --print-dead 2> /dev/null | wc -l || echo 0)

printf "  %-30s ${BOLD}%s${RESET}\n" "Total store size:" "$(human_size "$STORE_SIZE_KB")"
printf "  %-30s %s\n" "Store paths:" "$STORE_PATH_COUNT"
printf "  %-30s %s\n" "Dead (collectable):" "$DEAD_COUNT paths"

if [[ $DEAD_COUNT -eq 0 ]]; then
  info "GC has nothing left to collect. All paths are live."
else
  warn "$DEAD_COUNT dead paths remain. Run 'nix-store --gc' to reclaim."
fi

header "GC Roots Analysis"

nix-store --gc --print-roots 2> /dev/null > "$TMPDIR_AUDIT/all_roots.txt"

grep '/proc/' "$TMPDIR_AUDIT/all_roots.txt" > "$TMPDIR_AUDIT/proc_roots.txt" || true
grep -v '/proc/' "$TMPDIR_AUDIT/all_roots.txt" > "$TMPDIR_AUDIT/persist_roots.txt" || true

awk -F' -> ' '{print $2}' "$TMPDIR_AUDIT/persist_roots.txt" | sort -u > "$TMPDIR_AUDIT/persist_targets.txt"

PROC_ROOT_COUNT=$(wc -l < "$TMPDIR_AUDIT/proc_roots.txt")
PERSIST_ROOT_COUNT=$(wc -l < "$TMPDIR_AUDIT/persist_roots.txt")
PERSIST_TARGET_COUNT=$(wc -l < "$TMPDIR_AUDIT/persist_targets.txt")

printf "  %-30s %s\n" "Persistent GC roots:" "$PERSIST_ROOT_COUNT ($PERSIST_TARGET_COUNT unique targets)"
printf "  %-30s %s\n" "Process (/proc/) roots:" "$PROC_ROOT_COUNT"

declare -A ROOT_CATEGORIES
declare -A ROOT_LABELS
declare -A SEEN_STORE_PATHS

add_root() {
  local cat="$1" path="$2" label="$3"
  [[ -n ${SEEN_STORE_PATHS[$path]:-} ]] && return
  SEEN_STORE_PATHS[$path]=1
  if [[ -n ${ROOT_CATEGORIES[$cat]:-} ]]; then
    ROOT_CATEGORIES[$cat]+=$'\n'"$path"
  else
    ROOT_CATEGORIES[$cat]="$path"
  fi
  ROOT_LABELS[$path]="$label"
}

while IFS= read -r line; do
  source_path=$(echo "$line" | awk -F' -> ' '{print $1}' | xargs)
  store_path=$(echo "$line" | awk -F' -> ' '{print $2}' | xargs)

  case "$source_path" in
    */nix/var/nix/profiles/system*)
      add_root "system" "$store_path" "NixOS system ($(basename "$source_path"))"
      ;;
    *home-manager*)
      add_root "home-manager" "$store_path" "Home Manager"
      ;;
    *.direnv/flake-profile*)
      dir="${source_path%%/.direnv/*}"
      add_root "devshell" "$store_path" "devshell: $(basename "$dir")"
      ;;
    *.direnv/flake-inputs*)
      add_root "flake-inputs" "$store_path" "flake-input"
      ;;
    *nix/var/nix/profiles/per-user*)
      add_root "user-profile" "$store_path" "User profile ($(basename "$source_path"))"
      ;;
    */nix/var/nix/profiles/default*)
      add_root "default-profile" "$store_path" "Default profile"
      ;;
    */.cache/nix/flake-registry*) ;;
    */.local/state/nix/profiles/*)
      add_root "user-env" "$store_path" "User environment (nix profile)"
      ;;
    /run/booted-system)
      add_root "booted" "$store_path" "Booted system (pre-reboot)"
      ;;
    /run/current-system)
      add_root "system" "$store_path" "NixOS system (current-system)"
      ;;
    *)
      add_root "other" "$store_path" "Other: $(basename "$source_path")"
      ;;
  esac
done < "$TMPDIR_AUDIT/persist_roots.txt"

header "Closure Size Breakdown"

info "Computing closures... (this may take a moment)"
echo

true > "$TMPDIR_AUDIT/seen_paths.txt"

ORDERED_CATS=("system" "home-manager" "user-env" "default-profile" "user-profile" "devshell" "flake-inputs" "booted" "other")

declare -A CAT_TOTAL_KB
declare -A CAT_UNIQUE_KB
TOTAL_ACCOUNTED_KB=0

for cat in "${ORDERED_CATS[@]}"; do
  [[ -z ${ROOT_CATEGORIES[$cat]:-} ]] && continue

  if [[ $cat == "flake-inputs" ]]; then
    fi_paths="${ROOT_CATEGORIES[$cat]}"
    fi_count=$(echo "$fi_paths" | wc -l)
    {
      echo "$fi_paths"
      echo "$fi_paths" | xargs nix-store -q --requisites 2> /dev/null || true
    } | sort -u > "$TMPDIR_AUDIT/cur_reqs.txt"

    comm -23 "$TMPDIR_AUDIT/cur_reqs.txt" "$TMPDIR_AUDIT/seen_paths.txt" > "$TMPDIR_AUDIT/cur_unique.txt"
    unique_kb=$(paths_size_kb < "$TMPDIR_AUDIT/cur_unique.txt")
    unique_count=$(wc -l < "$TMPDIR_AUDIT/cur_unique.txt")

    sort -u -m "$TMPDIR_AUDIT/seen_paths.txt" "$TMPDIR_AUDIT/cur_reqs.txt" -o "$TMPDIR_AUDIT/seen_paths.txt"

    CAT_UNIQUE_KB[$cat]=$unique_kb
    TOTAL_ACCOUNTED_KB=$((TOTAL_ACCOUNTED_KB + unique_kb))

    printf "  ${BOLD}%-40s${RESET}  unique: ${BOLD}%-10s${RESET}  (%d inputs, %d unique paths)\n" \
      "Flake inputs (source trees)" "$(human_size "$unique_kb")" "$fi_count" "$unique_count"
    continue
  fi

  while IFS= read -r store_path; do
    [[ -z $store_path ]] && continue
    label="${ROOT_LABELS[$store_path]:-$store_path}"

    reqs "$store_path" > "$TMPDIR_AUDIT/cur_reqs.txt"

    echo "$store_path" >> "$TMPDIR_AUDIT/cur_reqs.txt"
    sort -u "$TMPDIR_AUDIT/cur_reqs.txt" -o "$TMPDIR_AUDIT/cur_reqs.txt"

    total_kb=$(paths_size_kb < "$TMPDIR_AUDIT/cur_reqs.txt")

    comm -23 "$TMPDIR_AUDIT/cur_reqs.txt" "$TMPDIR_AUDIT/seen_paths.txt" > "$TMPDIR_AUDIT/cur_unique.txt"
    unique_kb=$(paths_size_kb < "$TMPDIR_AUDIT/cur_unique.txt")
    unique_count=$(wc -l < "$TMPDIR_AUDIT/cur_unique.txt")

    sort -u -m "$TMPDIR_AUDIT/seen_paths.txt" "$TMPDIR_AUDIT/cur_reqs.txt" -o "$TMPDIR_AUDIT/seen_paths.txt"

    CAT_TOTAL_KB[$cat]=$((${CAT_TOTAL_KB[$cat]:-0} + total_kb))
    CAT_UNIQUE_KB[$cat]=$((${CAT_UNIQUE_KB[$cat]:-0} + unique_kb))
    TOTAL_ACCOUNTED_KB=$((TOTAL_ACCOUNTED_KB + unique_kb))

    printf "  ${BOLD}%-40s${RESET}  closure: %-10s  unique: ${BOLD}%-10s${RESET}  (%d paths)\n" \
      "$label" "$(human_size "$total_kb")" "$(human_size "$unique_kb")" "$unique_count"

    if [[ $unique_count -gt 0 ]] && [[ $unique_kb -gt 1024 ]]; then
      xargs du -s < "$TMPDIR_AUDIT/cur_unique.txt" 2> /dev/null \
        | sort -rn \
        | head -"$TOP_N" \
        | while read -r sz path; do
          printf "    ${DIM}%8s  %s${RESET}\n" "$(human_size "$sz")" "$(basename "$path")"
        done
    fi
  done <<< "${ROOT_CATEGORIES[$cat]}"
done

subhdr "Running processes (/proc/)"
awk -F' -> ' '{print $2}' "$TMPDIR_AUDIT/proc_roots.txt" | sort -u > "$TMPDIR_AUDIT/proc_targets.txt"
{
  cat "$TMPDIR_AUDIT/proc_targets.txt"
  xargs nix-store -q --requisites < "$TMPDIR_AUDIT/proc_targets.txt" 2> /dev/null || true
} | sort -u > "$TMPDIR_AUDIT/proc_all_reqs.txt"

comm -23 "$TMPDIR_AUDIT/proc_all_reqs.txt" "$TMPDIR_AUDIT/seen_paths.txt" > "$TMPDIR_AUDIT/proc_unique.txt"
proc_unique_kb=$(paths_size_kb < "$TMPDIR_AUDIT/proc_unique.txt")
proc_unique_count=$(wc -l < "$TMPDIR_AUDIT/proc_unique.txt")
TOTAL_ACCOUNTED_KB=$((TOTAL_ACCOUNTED_KB + proc_unique_kb))

printf "  %-40s  unique: ${BOLD}%-10s${RESET}  (%d paths)\n" \
  "$PROC_ROOT_COUNT process roots" "$(human_size "$proc_unique_kb")" "$proc_unique_count"

if [[ $proc_unique_count -gt 0 ]] && [[ $proc_unique_kb -gt 1024 ]]; then
  xargs du -s < "$TMPDIR_AUDIT/proc_unique.txt" 2> /dev/null \
    | sort -rn \
    | head -"$TOP_N" \
    | while read -r sz path; do
      printf "    ${DIM}%8s  %s${RESET}\n" "$(human_size "$sz")" "$(basename "$path")"
    done
fi

sort -u -m "$TMPDIR_AUDIT/seen_paths.txt" "$TMPDIR_AUDIT/proc_all_reqs.txt" -o "$TMPDIR_AUDIT/seen_paths.txt"

header "Summary"

UNACCOUNTED_KB=$((STORE_SIZE_KB - TOTAL_ACCOUNTED_KB))

printf "  ${BOLD}%-35s %10s${RESET}\n" "Root" "Unique Size"
hr

for cat in "${ORDERED_CATS[@]}"; do
  [[ -z ${CAT_UNIQUE_KB[$cat]:-} ]] && continue
  kb=${CAT_UNIQUE_KB[$cat]}
  ((kb < 1024)) && continue
  case "$cat" in
    system) lbl="NixOS system" ;;
    home-manager) lbl="Home Manager" ;;
    user-env) lbl="User environment (nix profile)" ;;
    default-profile) lbl="Default profile" ;;
    user-profile) lbl="User profiles" ;;
    devshell) lbl="Devshells (direnv)" ;;
    flake-inputs) lbl="Flake inputs (source trees)" ;;
    booted) lbl="Booted system (pre-reboot)" ;;
    other) lbl="Other roots" ;;
  esac
  printf "  %-35s %10s\n" "$lbl" "$(human_size "$kb")"
done

printf "  %-35s %10s\n" "Running processes" "$(human_size "$proc_unique_kb")"

if ((UNACCOUNTED_KB > 10240)); then
  printf "  ${DIM}%-35s %10s${RESET}\n" "Shared / fs overhead / small paths" "$(human_size "$UNACCOUNTED_KB")"
fi

hr
printf "  ${BOLD}%-35s %10s${RESET}\n" "Total store" "$(human_size "$STORE_SIZE_KB")"

header "Duplicate Packages"

info "Checking for multiple versions of the same package..."
echo

true > "$TMPDIR_AUDIT/dupe_candidates.txt"

du -sk /nix/store/*/ 2> /dev/null \
  | awk '$1 > 10240 {print}' \
  | while read -r sz fullpath; do
    name=$(basename "$fullpath" | sed 's/^[a-z0-9]\{32\}-//')

    case "$name" in
      *.drv | *-env | *-man | *-dev | *-doc | *-getent | *-info | source) continue ;;
    esac

    base=$(echo "$name" | sed -E 's/-(lib|debug|bin|static|libgcc)$//' | sed -E 's/-([0-9]+\.[0-9]+[0-9.]*).*$//')
    ver=$(echo "$name" | sed -E 's/-(lib|debug|bin|static|libgcc)$//' | grep -oP '[0-9]+\.[0-9]+[0-9.]*' | head -1)
    [[ -z $ver ]] && continue
    [[ -z $base ]] && continue
    echo -e "${sz}\t${base}\t${ver}\t${name}"
  done \
  | sort -t$'\t' -k2,2 -k3,3 -k1,1rn \
    > "$TMPDIR_AUDIT/dupe_candidates.txt" || true

awk -F'\t' '
  function flush() {
    if (base != "" && n_versions > 1) {
      printf "%d\t%s\t%s\t%s\n", total_sz, base, versions, entries
    }
  }
  {
    sz=$1; b=$2; v=$3; nm=$4
    if (b != base) {
      flush()
      base = b; n_versions = 0; total_sz = 0; versions = ""; entries = ""; prev_ver = ""
    }
    if (v != prev_ver) {
      n_versions++
      if (versions != "") versions = versions "," v; else versions = v

      total_sz += sz
      entry = sprintf("%8d\t%s", sz, nm)
      if (entries != "") entries = entries "|" entry; else entries = entry
      prev_ver = v
    }
  }
  END { flush() }
' "$TMPDIR_AUDIT/dupe_candidates.txt" \
  | sort -t$'\t' -k1,1rn \
  | head -10 \
    > "$TMPDIR_AUDIT/dupe_results.txt" || true

found_dupes=0
while IFS=$'\t' read -r total_sz base versions entries_raw; do
  [[ -z $base ]] && continue
  found_dupes=1
  printf "  ${YELLOW}%s${RESET} (${BOLD}%s wasted${RESET}, versions: %s)\n" \
    "$base" "$(human_size "$total_sz")" "$versions"
  IFS='|' read -ra entry_array <<< "$entries_raw"
  for entry in "${entry_array[@]}"; do
    sz=$(echo "$entry" | awk -F'\t' '{print $1}' | xargs)
    nm=$(echo "$entry" | awk -F'\t' '{print $2}')
    printf "    ${DIM}%8s  %s${RESET}\n" "$(human_size "$sz")" "$nm"
  done
done < "$TMPDIR_AUDIT/dupe_results.txt"

if [[ $found_dupes -eq 0 ]]; then
  good "No large duplicate packages detected (>50 MB)."
fi

header "Recommendations"

CURRENT_SYSTEM=$(readlink -f /nix/var/nix/profiles/system 2> /dev/null || true)
BOOTED_SYSTEM=$(readlink -f /run/booted-system 2> /dev/null || true)
if [[ -n $BOOTED_SYSTEM ]] && [[ $CURRENT_SYSTEM != "$BOOTED_SYSTEM" ]]; then
  warn "Reboot pending: /run/booted-system differs from current system profile."
  info "A reboot will release the old system closure and free /proc/ roots."
  echo
fi

if ((proc_unique_kb > 10240)); then
  warn "Running processes pin $(human_size "$proc_unique_kb") of unique store paths."
  info "Check long-running dev tools (editors, language servers, etc.)."

  awk -F' -> ' '{print $1}' "$TMPDIR_AUDIT/proc_roots.txt" \
    | grep -oP '/proc/\K[0-9]+' \
    | sort -u \
    | while read -r pid; do
      cmd=$(ps -p "$pid" -o comm= 2> /dev/null || echo "<exited>")
      echo "  PID $pid: $cmd"
    done \
    | sort -t: -k2 \
    | uniq -f1 -c \
    | sort -rn \
    | head -5 \
    | while read -r cnt rest; do
      printf "    ${DIM}%-6s %s${RESET}\n" "[x$cnt]" "$rest"
    done
  echo
fi

if ((DEAD_COUNT > 0)); then
  warn "$DEAD_COUNT dead paths exist. Run: nix-store --gc"
  echo
fi

for cat in "${ORDERED_CATS[@]}"; do
  [[ $cat != "devshell" ]] && continue
  [[ -z ${CAT_UNIQUE_KB[$cat]:-} ]] && continue
  kb=${CAT_UNIQUE_KB[$cat]}
  if ((kb > 2097152)); then
    warn "Devshell closures add $(human_size "$kb") on top of system."
    info "Review flake devShell inputs for unnecessary large dependencies."
  fi
done

good "Run 'nix-store --gc' after closing dev shells and rebooting."
good "Use 'nix-store --optimise' to hardlink identical files (saves 5-15%)."
good "Remove old .direnv/ in unused project directories to drop devshell roots."

echo
