#!/usr/bin/env python3
# ruff: noqa E402

import argparse
import json
import logging
import os
import re
import stat
import subprocess
import tempfile
import time

import gi

gi.require_version("GUdev", "1.0")

from gi.repository import GLib, GUdev

LOG_FILE_PATH = os.path.expanduser("~/.cache/auto_refresh/log.txt")
CONFIG_FILE_PATH = os.path.expanduser("~/.config/hypr/monitors.conf")
AC_STATUS_FILE_PATH = "/sys/class/power_supply/AC/online"
if not os.path.exists(AC_STATUS_FILE_PATH):
    AC_STATUS_FILE_PATH = "/sys/class/power_supply/ACAD/online"

TARGET_MONITOR = ""
MAX_REFRESH_RATE = 0
MIN_REFRESH_RATE = 0

os.makedirs(os.path.dirname(LOG_FILE_PATH), exist_ok=True)
logging.basicConfig(
    filename=LOG_FILE_PATH,
    level=logging.INFO,
    format="%(asctime)s - %(levelname)s - %(message)s",
)


def detect_edp_monitor() -> dict:
    """Auto-detect the built-in laptop display (eDP-*) via hyprctl monitors."""
    try:
        result = subprocess.run(
            ["hyprctl", "monitors", "-j"],
            check=True,
            capture_output=True,
            text=True,
        )
        monitors = json.loads(result.stdout)
    except (subprocess.CalledProcessError, FileNotFoundError, json.JSONDecodeError) as e:
        logging.error(f"Failed to query hyprctl monitors: {e}")
        return {}

    for mon in monitors:
        if mon.get("name", "").startswith("eDP-"):
            width = mon.get("width", 0)
            height = mon.get("height", 0)
            available_modes = mon.get("availableModes", [])

            rates = []
            for mode in available_modes:
                m = re.match(rf"^{width}x{height}@([\d.]+)Hz$", mode)
                if m:
                    rates.append(float(m.group(1)))

            if not rates:
                logging.error(
                    f"No available modes found for eDP monitor '{mon['name']}'"
                )
                return {}

            name = mon["name"]
            make = mon.get("make", "")
            model = mon.get("model", "")
            desc = f"desc:{make} {model}".strip()

            # Figure out which identifier the config file actually uses
            target = name
            try:
                with open(CONFIG_FILE_PATH) as f:
                    config_content = f.read()
                if desc in config_content:
                    target = desc
                elif name in config_content:
                    target = name
                else:
                    logging.warning(
                        f"Neither '{desc}' nor '{name}' found in {CONFIG_FILE_PATH}."
                    )
            except FileNotFoundError:
                logging.warning(f"Config file not found at {CONFIG_FILE_PATH}, using '{name}'.")

            return {
                "target_monitor": target,
                "max_refresh_rate": int(max(rates)),
                "min_refresh_rate": int(min(rates)),
            }

    logging.error("No eDP (built-in laptop) monitor found.")
    return {}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Auto-switch monitor refresh rate based on AC power status (Hyprland).",
    )
    parser.add_argument(
        "--monitor",
        type=str,
        default=None,
        help='Monitor identifier as used in monitors.conf (e.g. "desc:Samsung Display Corp. 0x41AA"). '
        "Auto-detected from eDP if not specified.",
    )
    parser.add_argument(
        "--max-rate",
        type=int,
        default=None,
        help="Refresh rate to use on AC power. Auto-detected if not specified.",
    )
    parser.add_argument(
        "--min-rate",
        type=int,
        default=None,
        help="Refresh rate to use on battery. Auto-detected if not specified.",
    )
    return parser.parse_args()


def ensure_writable_config(path: str):
    """If path is a symlink or read-only, replace it with a writable copy.

    Reads the content from the (possibly symlinked/read-only) file,
    writes it to a new temp file in the same directory, then atomically
    swaps it into place via os.rename().
    """
    needs_replace = os.path.islink(path)
    if not needs_replace:
        try:
            needs_replace = not os.access(path, os.W_OK)
        except OSError:
            needs_replace = True

    if not needs_replace:
        return

    logging.info(
        f"Config file at {path} is {'a symlink' if os.path.islink(path) else 'read-only'}. "
        "Replacing with a writable copy."
    )

    try:
        with open(path) as f:
            content = f.read()
    except Exception as e:
        logging.error(f"Failed to read config for copy: {e}")
        raise

    config_dir = os.path.dirname(path)
    fd, tmp_path = tempfile.mkstemp(dir=config_dir, prefix=".monitors_", suffix=".conf")
    try:
        os.write(fd, content.encode())
        os.fchmod(fd, stat.S_IRUSR | stat.S_IWUSR | stat.S_IRGRP | stat.S_IROTH)  # 0o644
        os.close(fd)

        if os.path.islink(path):
            os.unlink(path)

        os.rename(tmp_path, path)
        logging.info(f"Replaced {path} with a writable copy.")
    except Exception as e:
        logging.error(f"Failed to replace config file: {e}")
        try:
            os.close(fd)
        except OSError:
            pass
        try:
            os.unlink(tmp_path)
        except OSError:
            pass
        raise


def sed(regex: str, path: str):
    """Runs a sed command and logs its outcome."""
    command = ["sed", "-i", regex, path]
    logging.info(f"Running command: {' '.join(command)}")
    try:
        result = subprocess.run(command, check=True, capture_output=True, text=True)
        logging.info("Command executed successfully.")
        if result.stderr:
            logging.warning(f"[sed stderr]: {result.stderr.strip()}")
    except FileNotFoundError:
        logging.error(
            "sed command not found. Please ensure 'sed' is installed and in your PATH."
        )
    except subprocess.CalledProcessError as e:
        logging.error(f"Command failed with exit code {e.returncode}")
        logging.error(f"[sed stdout]: {e.stdout.strip()}")
        logging.error(f"[sed stderr]: {e.stderr.strip()}")
    except Exception as e:
        logging.exception(f"An unexpected error occurred while running sed: {e}")


def reload_hyprland():
    """Runs 'hyprctl reload' and logs its outcome."""
    command = ["hyprctl", "reload"]
    logging.info(f"Running command: {' '.join(command)}")
    try:
        result = subprocess.run(command, check=True, capture_output=True, text=True)
        logging.info("Hyprland reloaded successfully.")
        if result.stdout:
            logging.info(f"[hyprctl stdout]: {result.stdout.strip()}")
        if result.stderr:
            logging.warning(f"[hyprctl stderr]: {result.stderr.strip()}")
    except FileNotFoundError:
        logging.error(
            "hyprctl command not found. Please ensure Hyprland is running and 'hyprctl' is in your PATH."
        )
    except subprocess.CalledProcessError as e:
        logging.error(f"Command failed with exit code {e.returncode}")
        logging.error(f"[hyprctl stdout]: {e.stdout.strip()}")
        logging.error(f"[hyprctl stderr]: {e.stderr.strip()}")
    except Exception as e:
        logging.exception(f"An unexpected error occurred while running hyprctl: {e}")


def set_refresh_rate(target_rate: int):
    """Sets the refresh rate by modifying the config file, but only if the TARGET_MONITOR is found."""
    if target_rate == MAX_REFRESH_RATE:
        from_rate, to_rate = MIN_REFRESH_RATE, MAX_REFRESH_RATE
        logging.info(
            f"AC power connected. Attempting to set refresh rate to max for {TARGET_MONITOR}."
        )
    elif target_rate == MIN_REFRESH_RATE:
        from_rate, to_rate = MAX_REFRESH_RATE, MIN_REFRESH_RATE
        logging.info(
            f"AC power disconnected. Attempting to set refresh rate to min for {TARGET_MONITOR}."
        )
    else:
        logging.error(f"Invalid target rate: {target_rate}")
        return

    try:
        ensure_writable_config(CONFIG_FILE_PATH)

        with open(CONFIG_FILE_PATH) as file:
            file_lines = file.readlines()

        target_line = None
        for line in file_lines:
            if TARGET_MONITOR in line:
                target_line = line
                break

        if target_line:
            if f"@{from_rate}" in target_line:
                regex = f"/{TARGET_MONITOR}/s/@{from_rate}/@{to_rate}/g"
                sed(regex, CONFIG_FILE_PATH)
                reload_hyprland()
            else:
                logging.info(
                    f"Refresh rate for {TARGET_MONITOR} is not @{from_rate}. No change needed."
                )
        else:
            logging.warning(
                f"Target '{TARGET_MONITOR}' not found in {CONFIG_FILE_PATH}. No changes made."
            )

    except FileNotFoundError:
        logging.error(f"Config file not found at: {CONFIG_FILE_PATH}")
    except Exception as e:
        logging.exception(f"Failed to read config file: {e}")


def check_initial_power_status():
    """Initially, AC status can be unreliable. Retry until it is valid."""
    while True:
        try:
            with open(AC_STATUS_FILE_PATH) as file:
                online = file.read().strip()
                logging.info(f"[Initial check] Raw power status is '{online}'")
                if online == "0":
                    set_refresh_rate(MIN_REFRESH_RATE)
                    break
                elif online == "1":
                    set_refresh_rate(MAX_REFRESH_RATE)
                    break
                else:
                    logging.warning(
                        f"[Initial check] Invalid status: {online}. Retrying..."
                    )
        except FileNotFoundError:
            logging.error(
                f"AC status file not found at {AC_STATUS_FILE_PATH}. Exiting."
            )
            exit(1)
        except Exception as e:
            logging.exception(f"Error during initial check: {e}. Retrying...")

        time.sleep(1)


def ac_event_handler(client, action, device, user_data):
    if action == "change" and device.get_property("SUBSYSTEM") == "power_supply":
        online = device.get_property("POWER_SUPPLY_ONLINE")
        logging.info(f"[AC event] online status is: {online}")

        if online == "1":
            set_refresh_rate(MAX_REFRESH_RATE)
        elif online == "0":
            set_refresh_rate(MIN_REFRESH_RATE)

        if device.get_property("POWER_SUPPLY_CAPACITY_LEVEL") == "critical":
            logging.warning("Battery level is critical!")
            subprocess.run(
                [
                    "notify-send",
                    "--urgency=critical",
                    "Battery critical!",
                ]
            )


def main():
    global TARGET_MONITOR, MAX_REFRESH_RATE, MIN_REFRESH_RATE

    args = parse_args()
    detected = detect_edp_monitor()

    TARGET_MONITOR = args.monitor or detected.get("target_monitor", "")
    MAX_REFRESH_RATE = args.max_rate or detected.get("max_refresh_rate", 0)
    MIN_REFRESH_RATE = args.min_rate or detected.get("min_refresh_rate", 0)

    if not TARGET_MONITOR or not MAX_REFRESH_RATE or not MIN_REFRESH_RATE:
        logging.error(
            "Could not determine monitor configuration. "
            "Either pass --monitor, --max-rate, --min-rate or ensure an eDP monitor is connected."
        )
        print(
            "Error: Could not auto-detect monitor. "
            "Use --monitor, --max-rate, --min-rate to specify manually."
        )
        exit(1)

    logging.info("--- Auto Refresh Rate Service Started ---")
    logging.info(
        f"Monitor: {TARGET_MONITOR}, Max: {MAX_REFRESH_RATE}Hz, Min: {MIN_REFRESH_RATE}Hz"
    )
    client = GUdev.Client(subsystems=["power_supply"])
    check_initial_power_status()
    client.connect("uevent", ac_event_handler, None)
    loop = GLib.MainLoop()
    try:
        loop.run()
    except KeyboardInterrupt:
        logging.info("--- Service stopped by user ---")
        pass


if __name__ == "__main__":
    """
    Monitors power supply events and automatically changes monitor refresh rate.
    Only Hyprland Window Manager is supported.
    """
    main()
