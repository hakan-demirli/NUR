#!/usr/bin/env bash
set -euo pipefail

readonly LIBEXEC_DIR="@libexec@"

die() {
  printf "error: %s\n" "$*" >&2
  exit 1
}
info() { printf ":: %s\n" "$*"; }

check_deps() {
  local missing=()
  for cmd in wine innoextract find md5sum; do
    command -v "$cmd" > /dev/null 2>&1 || missing+=("$cmd")
  done
  [[ ${#missing[@]} -eq 0 ]] || die "missing dependencies: ${missing[*]}"
}

usage() {
  cat << EOF
Usage: $(basename "$0") <command> [options]

Extract FitGirl Repacks on Linux using Wine.

Commands:
  extract <repack-dir> -o <output-dir>   Extract a repack
  verify  <repack-dir>                   Verify bin checksums

Extract options:
  --no-mover    Skip running mover/*.bat post-install scripts

Examples:
  $(basename "$0") extract "/mnt/games/Some Game [FitGirl Repack]" -o ~/Games/some-game
  $(basename "$0") verify  "/mnt/games/Some Game [FitGirl Repack]"
EOF
  exit 0
}

find_setup() {
  local dir="$1"
  for f in "$dir"/setup.exe "$dir"/Setup.exe "$dir"/SETUP.EXE; do
    [[ -f $f ]] && printf "%s" "$f" && return 0
  done
  return 1
}

find_bins() {
  local dir="$1"
  local bins=()
  for f in "$dir"/fg-*.bin; do
    [[ -f $f ]] && bins+=("$f")
  done
  [[ ${#bins[@]} -gt 0 ]] || return 1
  printf "%s\n" "${bins[@]}"
}

extract_tools() {
  local workdir="$1" setup_exe="$2"
  info "Extracting tools from setup.exe..."

  local tmpdir="$workdir/inno-dump"
  mkdir -p "$tmpdir"
  innoextract --dump -d "$tmpdir" "$setup_exe" > /dev/null 2>&1

  mkdir -p "$workdir/tools"
  find "$tmpdir" -maxdepth 1 -type f | while IFS= read -r f; do
    local name
    name="$(basename "$f")"
    name="${name#\{tmp\}\\}"
    name="${name#\{app\}\\}"
    cp "$f" "$workdir/tools/$name"
  done
  rm -rf "$tmpdir"

  cp "$LIBEXEC_DIR/unarc.exe" "$workdir/tools/unarc.exe"
  cp "$LIBEXEC_DIR/CLS-srep.dll" "$workdir/tools/CLS-srep.dll"

  if [[ -f "$workdir/tools/CLS.ini" ]]; then
    sed -i 's|TempPath={app}[^ ]*|TempPath=.\\temp|g' "$workdir/tools/CLS.ini"
    sed -i 's|TmpPath={app}[^ ]*|TmpPath=.\\temp|g' "$workdir/tools/CLS.ini"
    sed -i 's|ldmfTempPath={app}[^ ]*|ldmfTempPath=.\\temp|g' "$workdir/tools/CLS.ini"
  fi

  info "Tools ready ($(find "$workdir/tools" \( -name '*.dll' -o -name '*.exe' \) | wc -l) files)"
}

extract_archives() {
  local workdir="$1" outdir="$2"
  shift 2
  local bin_files=("$@")

  mkdir -p "$outdir"

  local win_outdir="Z:${outdir//\//\\}"
  local total=${#bin_files[@]}
  local i=0
  local pids=()
  local logs=()

  for bin in "${bin_files[@]}"; do
    i=$((i + 1))
    local name
    name="$(basename "$bin")"
    local win_bin="Z:${bin//\//\\}"

    local tmpdir="$workdir/temp-$i"
    mkdir -p "$tmpdir"
    local win_tmpdir="Z:${tmpdir//\//\\}"
    local logfile="$workdir/log-$name.txt"

    info "[$i/$total] Starting $name..."

    (
      cd "$workdir/tools"
      WINEDEBUG=-all wine unarc.exe unarc.dll x \
        -o+ \
        "-dp${win_outdir}" \
        "-w${win_tmpdir}" \
        -cfgarc.ini \
        "$win_bin" \
        "" 2>&1
    ) > "$logfile" 2>&1 &

    pids+=($!)
    logs+=("$logfile")
  done

  info "All $total archives started in parallel. Waiting..."

  local failed=0
  for j in "${!pids[@]}"; do
    local pid=${pids[$j]}
    local log=${logs[$j]}
    local binname
    binname="$(basename "${bin_files[$j]}")"

    if wait "$pid"; then
      if grep -qi "error" "$log" 2> /dev/null; then
        info "  $binname: completed with warnings"
        grep -i "error" "$log" | head -3
      else
        info "  $binname: done"
      fi
    else
      info "  $binname: FAILED (exit $?)"
      tail -5 "$log"
      failed=$((failed + 1))
    fi
  done

  [[ $failed -eq 0 ]] || die "$failed archive(s) failed to extract. Check logs in $workdir/"
  info "Extraction complete."
}

run_movers() {
  local outdir="$1"

  if [[ ! -d "$outdir/mover" ]]; then
    info "No mover/ directory found, skipping post-install moves."
    return 0
  fi

  info "Running post-install mover scripts..."

  local moved=0
  for batfile in "$outdir"/mover/*.bat; do
    [[ -f $batfile ]] || continue
    info "  Processing $(basename "$batfile")..."

    while IFS= read -r line; do
      if [[ $line =~ ^move\ +\"([^\"]+)\"\ +\"([^\"]+)\" ]]; then
        local src="${BASH_REMATCH[1]//\\//}"
        local dst="${BASH_REMATCH[2]//\\//}"
        local full_src="$outdir/$src"
        local full_dst="$outdir/$dst"

        if [[ -f $full_src ]]; then
          mkdir -p "$(dirname "$full_dst")"
          mv "$full_src" "$full_dst"
          moved=$((moved + 1))
        fi
      fi
    done < <(tr -d '\r' < "$batfile")
  done

  rmdir "$outdir/temp" 2> /dev/null || true
  info "Post-install moves complete ($moved files moved)."
}

verify_bins() {
  local repackdir="$1"

  local md5file=""
  for f in "$repackdir"/MD5/*.md5 "$repackdir"/md5/*.md5 "$repackdir"/*.md5; do
    [[ -f $f ]] && md5file="$f" && break
  done

  [[ -n $md5file ]] || die "no MD5 checksum file found in $repackdir"

  info "Verifying checksums from $(basename "$md5file")..."

  local total=0 passed=0 failed=0

  while IFS= read -r line; do
    [[ $line =~ ^[[:space:]]*$ ]] && continue
    [[ $line =~ ^[\;\#] ]] && continue

    local hash="" fname=""
    if [[ $line =~ ^([0-9a-fA-F]+)\ +\*?\.\.\\?(.+)$ ]]; then
      hash="${BASH_REMATCH[1]}"
      fname="${BASH_REMATCH[2]}"
    elif [[ $line =~ ^([0-9a-fA-F]+)\ +\*?(.+)$ ]]; then
      hash="${BASH_REMATCH[1]}"
      fname="${BASH_REMATCH[2]}"
    else
      continue
    fi

    fname="${fname//\\//}"
    local filepath="$repackdir/$fname"

    if [[ ! -f $filepath ]]; then
      info "  SKIP: $fname (not found)"
      continue
    fi

    total=$((total + 1))
    local actual
    actual="$(md5sum "$filepath" | cut -d' ' -f1)"

    if [[ $actual == "$hash" ]]; then
      info "  OK:   $fname"
      passed=$((passed + 1))
    else
      info "  FAIL: $fname (expected $hash, got $actual)"
      failed=$((failed + 1))
    fi
  done < <(tr -d '\r' < "$md5file")

  info "Verification: $passed/$total passed, $failed failed."
  [[ $failed -eq 0 ]] || die "MD5 verification failed for $failed file(s)."
}

cmd_extract() {
  local repack_dir="" output_dir="" no_mover=0

  while [[ $# -gt 0 ]]; do
    case "$1" in
      -o | --output)
        output_dir="$2"
        shift 2
        ;;
      --no-mover)
        no_mover=1
        shift
        ;;
      *)
        if [[ -z $repack_dir ]]; then
          repack_dir="$1"
          shift
        else
          die "unknown argument: $1"
        fi
        ;;
    esac
  done

  [[ -n $repack_dir ]] || die "missing <repack-dir>. Run: $(basename "$0") help"
  [[ -n $output_dir ]] || die "missing -o <output-dir>. Run: $(basename "$0") help"
  [[ -d $repack_dir ]] || die "repack directory not found: $repack_dir"

  repack_dir="$(realpath "$repack_dir")"
  output_dir="$(realpath -m "$output_dir")"

  local setup_exe
  setup_exe=$(find_setup "$repack_dir") || die "no setup.exe found in $repack_dir"

  local bin_files=()
  mapfile -t bin_files < <(find_bins "$repack_dir") || die "no fg-*.bin files found in $repack_dir"

  info "Found ${#bin_files[@]} archive(s) in: $repack_dir"
  check_deps

  local workdir
  workdir="$(mktemp -d -t fitgirl-extract.XXXXXX)"
  trap 'rm -rf "$workdir"' EXIT

  extract_tools "$workdir" "$setup_exe"
  extract_archives "$workdir" "$output_dir" "${bin_files[@]}"

  if [[ $no_mover -eq 0 ]]; then
    run_movers "$output_dir"
  fi

  info "Done! Game extracted to: $output_dir"
}

cmd_verify() {
  local repack_dir="${1:-}"
  [[ -n $repack_dir ]] || die "missing <repack-dir>. Run: $(basename "$0") verify <repack-dir>"
  [[ -d $repack_dir ]] || die "repack directory not found: $repack_dir"
  repack_dir="$(realpath "$repack_dir")"
  verify_bins "$repack_dir"
}

main() {
  case "${1:-}" in
    extract)
      shift
      cmd_extract "$@"
      ;;
    verify)
      shift
      cmd_verify "$@"
      ;;
    help | -h | --help) usage ;;
    "") usage ;;
    *) die "unknown command: $1. Run: $(basename "$0") help" ;;
  esac
}

main "$@"
