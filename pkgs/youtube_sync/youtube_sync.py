#!/usr/bin/env python3

import argparse
import configparser
import contextlib
import json
import logging
import os
import shutil
import sys
import tempfile
import time
from concurrent.futures import ThreadPoolExecutor, as_completed
from dataclasses import dataclass
from multiprocessing import Pool, cpu_count
from pathlib import Path

import yt_dlp


def get_xdg_cache_home():
    return Path(os.environ.get("XDG_CACHE_HOME", os.path.expanduser("~/.cache")))


def setup_run_logging() -> tuple[Path, logging.Logger]:
    cache_home = get_xdg_cache_home()
    base_dir = cache_home / "youtube_sync"
    timestamp = str(int(time.time()))
    run_dir = base_dir / timestamp
    run_dir.mkdir(parents=True, exist_ok=True)

    latest_link = base_dir / "latest"
    if latest_link.exists() or latest_link.is_symlink():
        latest_link.unlink()
    latest_link.symlink_to(run_dir)

    log_file = run_dir / "orchestrator.log"

    logger = logging.getLogger("orchestrator")
    logger.setLevel(logging.DEBUG)

    fh = logging.FileHandler(log_file)
    fh.setFormatter(logging.Formatter("%(asctime)s [%(levelname)s] %(message)s"))
    logger.addHandler(fh)

    ch = logging.StreamHandler(sys.stdout)
    ch.setLevel(logging.INFO)
    ch.setFormatter(logging.Formatter("%(message)s"))
    logger.addHandler(ch)

    return run_dir, logger


def get_playlist_logger(
    run_dir: Path, playlist_name: str, stage: str
) -> logging.Logger:
    playlist_log_dir = run_dir / playlist_name
    playlist_log_dir.mkdir(parents=True, exist_ok=True)

    logger_name = f"{playlist_name}.{stage}"
    logger = logging.getLogger(logger_name)
    logger.setLevel(logging.DEBUG)
    logger.propagate = False

    if not logger.handlers:
        log_file = playlist_log_dir / f"{stage}.log"
        fh = logging.FileHandler(log_file)
        fh.setFormatter(logging.Formatter("%(asctime)s [%(levelname)s] %(message)s"))
        logger.addHandler(fh)

    return logger


@dataclass
class VideoTask:
    id: str
    url: str
    title: str
    playlist_dir: str
    playlist_name: str
    index: int
    duration: int
    channel: str
    run_dir: Path


@dataclass
class SyncResult:
    success: bool
    message: str
    task: VideoTask | None = None


def sanitize_filename(name: str) -> str:
    keep = (" ", ".", "_", "-")
    return (
        "".join(c for c in name if c.isalnum() or c in keep).strip().replace(" ", "_")
    )


def scan_playlist(args: tuple[str, str, str, Path]) -> list[VideoTask]:
    playlist_name, playlist_path, playlist_url, run_dir = args

    logger = get_playlist_logger(run_dir, playlist_name, "playlist_diff")
    logger.info(f"START: Scanning playlist '{playlist_name}'")
    logger.info(f"URL: {playlist_url}")
    logger.info(f"Target: {playlist_path}")

    ydl_opts = {
        "extract_flat": True,
        "dump_single_json": True,
        "ignoreerrors": True,
        "quiet": True,
        "no_warnings": True,
        "logger": logger,
    }

    try:
        with yt_dlp.YoutubeDL(ydl_opts) as ydl:
            data = ydl.extract_info(playlist_url, download=False)
    except Exception as e:
        logger.error(f"Failed to scan playlist: {e}")
        return []

    entries = data.get("entries", [])
    if not entries:
        logger.warning("No entries found")
        return []

    logger.info(f"Found {len(entries)} remote entries.")

    archive_file = Path(playlist_path) / "downloaded.txt"
    downloaded_ids = set()
    if archive_file.exists():
        with open(archive_file) as f:
            for line in f:
                parts = line.split()
                if len(parts) >= 2:
                    downloaded_ids.add(parts[1])

    logger.info(f"Found {len(downloaded_ids)} locally archived items.")

    missing_tasks = []

    for idx, entry in enumerate(entries, start=1):
        if entry is None:
            logger.warning(f"SKIP: null entry at index {idx}")
            continue

        vid_id = entry.get("id")
        if not vid_id:
            logger.warning(f"SKIP: entry with no id at index {idx}")
            continue

        # NOTE: dict.get(key, default) only returns the default when the key
        title = entry.get("title") or "Unknown"

        if vid_id in downloaded_ids:
            continue

        task = VideoTask(
            id=vid_id,
            url=entry.get("url")
            or entry.get("webpage_url")
            or f"https://www.youtube.com/watch?v={vid_id}",
            title=title,
            playlist_dir=playlist_path,
            playlist_name=playlist_name,
            index=idx,
            duration=entry.get("duration", 0),
            channel=entry.get("uploader", "Unknown"),
            run_dir=run_dir,
        )
        missing_tasks.append(task)
        logger.info(f"MISSING: [{idx}] {title} ({vid_id})")

    logger.info(f"Scan complete. {len(missing_tasks)} items to fetch.")
    return missing_tasks


def fetch_video(task: VideoTask) -> SyncResult:

    playlist_log_dir = task.run_dir / task.playlist_name
    playlist_log_dir.mkdir(parents=True, exist_ok=True)
    log_file = playlist_log_dir / "fetch.log"

    def log_msg(level, msg):
        timestamp = time.strftime("%Y-%m-%d %H:%M:%S")
        title_tag = (task.title or "Unknown")[:20]
        entry = f"{timestamp} [{level}] [{title_tag}] {msg}\n"
        try:
            with open(log_file, "a") as f:
                f.write(entry)
        except Exception:
            pass

    temp_dir: Path | None = None

    try:
        temp_dir = (
            Path(tempfile.gettempdir()) / "youtube_sync" / task.run_dir.name / task.id
        )
        temp_dir.mkdir(parents=True, exist_ok=True)

        log_msg("INFO", f"START download to {temp_dir}")

        safe_title = sanitize_filename(task.title or "Unknown")
        filename_tmpl = f"{task.index:02d}_{safe_title}.%(ext)s"

        ydl_opts = {
            "outtmpl": str(temp_dir / filename_tmpl),
            "format": "bestaudio",
            "postprocessors": [
                {
                    "key": "FFmpegExtractAudio",
                    "preferredcodec": "opus",
                    "preferredquality": "0",
                },
                {"key": "EmbedThumbnail"},
                {"key": "FFmpegMetadata"},
            ],
            "writethumbnail": True,
            "quiet": True,
            "no_warnings": True,
            "ignoreerrors": True,
        }

        max_retries = 3
        success = False

        for attempt in range(max_retries):
            try:
                with yt_dlp.YoutubeDL(ydl_opts) as ydl:
                    ydl.download([task.url])
                success = True
                break
            except Exception as e:
                log_msg("WARNING", f"Attempt {attempt + 1} failed: {e}")
                time.sleep(2)

        if not success:
            log_msg("ERROR", "Failed after max retries")
            return SyncResult(False, "Failed after max retries", task)

        downloaded_files = list(temp_dir.glob("*.opus"))
        if not downloaded_files:
            log_msg("ERROR", "No .opus file found after download")
            return SyncResult(False, "No .opus file found", task)

        src_file = downloaded_files[0]
        dest_dir = Path(task.playlist_dir)
        final_dest_file = dest_dir / src_file.name
        temp_dest_file = dest_dir / f".{src_file.name}.tmp_install"

        log_msg("INFO", f"Installing to {final_dest_file}")

        shutil.copy2(str(src_file), str(temp_dest_file))

        os.rename(str(temp_dest_file), str(final_dest_file))

        log_msg("INFO", "SUCCESS")
        return SyncResult(True, "Success", task)

    except Exception as e:
        # log_msg must never raise here, or the worker dies and
        # Pool.imap_unordered abandons every remaining task.
        with contextlib.suppress(Exception):
            log_msg("CRITICAL", f"Unexpected error: {e}")
        return SyncResult(False, str(e), task)
    finally:
        if temp_dir is not None and temp_dir.exists():
            shutil.rmtree(temp_dir, ignore_errors=True)


def discover_playlists(
    root_path: str, orchestrator_logger
) -> list[tuple[str, str, str]]:
    gitmodules = Path(root_path) / ".gitmodules"
    if not gitmodules.exists():
        orchestrator_logger.error(f"No .gitmodules found at {root_path}")
        return []

    config = configparser.ConfigParser()
    config.read(gitmodules)

    playlists = []
    for section in config.sections():
        if config.has_option(section, "path"):
            rel_path = config.get(section, "path")
            full_path = Path(root_path) / rel_path
            metadata_path = full_path / "metadata.json"

            if metadata_path.exists():
                try:
                    with open(metadata_path) as f:
                        meta = json.load(f)
                        url = meta.get("playlist_url")
                        if url:
                            playlists.append((rel_path, str(full_path), url))
                except Exception as e:
                    orchestrator_logger.error(
                        f"Error reading metadata for {rel_path}: {e}"
                    )
            else:
                orchestrator_logger.warning(f"No metadata.json in {rel_path}")

    return playlists


def create_m3u8(directory: str):
    p = Path(directory)
    if not p.exists():
        return

    entries = set()
    for f in p.glob("*.opus"):
        entries.add(f.name)

    for f in p.glob("*.opus.partaa"):
        entries.add(f.stem)

    if not entries:
        return

    playlist_file = p / f"{p.name}.m3u8"
    with open(playlist_file, "w") as f:
        for audio in sorted(entries):
            f.write(f"./{audio}\n")


def main():
    parser = argparse.ArgumentParser(description="Graph-based Atomic YouTube Sync")
    parser.add_argument(
        "--music-dir",
        default=str(Path.home() / ".local/share/sounds"),
        help="Root music directory",
    )
    parser.add_argument(
        "--workers", type=int, default=cpu_count(), help="Parallel downloads"
    )
    args = parser.parse_args()

    run_dir, log = setup_run_logging()

    log.info("--- YouTube Sync Orchestrator ---")
    log.info(f"Run ID: {run_dir.name}")
    log.info(f"Log Dir: {run_dir}")
    log.info(f"Music Dir: {args.music_dir}")

    log.info("[Phase 1] Discovering Playlists...")
    playlists = discover_playlists(args.music_dir, log)
    if not playlists:
        log.info("No playlists found. Exiting.")
        return

    log.info(f"Scanning {len(playlists)} playlists for updates...")
    all_tasks = []

    scan_args = [(p[0], p[1], p[2], run_dir) for p in playlists]

    with ThreadPoolExecutor(max_workers=len(playlists)) as executor:
        futures = {executor.submit(scan_playlist, arg): arg[0] for arg in scan_args}

        for future in as_completed(futures):
            p_name = futures[future]
            try:
                res = future.result()
                all_tasks.extend(res)
            except Exception as e:
                log.error(f"Scanner crashed for {p_name}: {e}")

    if not all_tasks:
        log.info("[Phase 2] No new videos to fetch. System is up to date.")
    else:
        log.info(f"[Phase 2] Realisation Graph Built: {len(all_tasks)} missing items.")
        log.info(f"Starting Worker Pool ({args.workers} workers)...")

        success_count = 0
        fail_count = 0

        with Pool(processes=args.workers) as pool:
            results = pool.imap_unordered(fetch_video, all_tasks)

            total = len(all_tasks)
            for i, res in enumerate(results, 1):
                if res.success:
                    success_count += 1
                    status = "OK "

                    try:
                        vid_id = (
                            res.task.url.split("v=")[-1]
                            if "v=" in res.task.url
                            else res.task.url.split("/")[-1]
                        )
                        archive_line = f"youtube {vid_id}\n"
                        dest_dir = Path(res.task.playlist_dir)
                        with open(dest_dir / "downloaded.txt", "a") as f:
                            f.write(archive_line)
                    except Exception as e:
                        log.error(f"Accounting failed for {res.task.title}: {e}")

                else:
                    fail_count += 1
                    status = "ERR"

                sys.stdout.write(
                    f"\r[{i}/{total}] {status} | {res.task.playlist_name[:15]:<15} | {res.task.title[:30]:<30}"
                )
                sys.stdout.flush()

                if not res.success:
                    log.error(
                        f"Task Failed: {res.task.playlist_name}/{res.task.title} - {res.message}"
                    )

        print("")
        log.info(f"Sync Complete. Success: {success_count}, Failures: {fail_count}")

    log.info("[Phase 3] Regenerating M3U8 Playlists...")
    for _, path, _ in playlists:
        create_m3u8(path)

    log.info("All operations completed.")


if __name__ == "__main__":
    main()
