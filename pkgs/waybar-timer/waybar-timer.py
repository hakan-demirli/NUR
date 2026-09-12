#!/usr/bin/env python3
import argparse
import json
import logging
import os
import subprocess
from datetime import datetime, timedelta
from enum import Enum

TIMER_FILE = "/tmp/waybar-timer.json"
DEFAULT_MINUTE = 40
FONT_SIZE = 14
ZENITY = "zenity"

xdg_cache_dir = os.environ.get("XDG_CACHE_HOME", os.path.expanduser("~/.cache"))
log_file_path = os.path.join(xdg_cache_dir, "waybar-timer.log")
logging.basicConfig(
    filename=log_file_path,
    level=logging.INFO,
    format="%(asctime)s:%(levelname)s:%(message)s",
)


class TimerState(Enum):
    READY = "ready"
    COUNTING = "counting"
    STOPPED = "stopped"
    TIMEOUT = "timeout"
    STOPWATCH = "stopwatch"  # New state for stopwatch
    STOPWATCH_PAUSED = "stopwatch_paused"  # Paused stopwatch state


def play_sound(file: str):
    sound_path = os.path.join(
        os.environ.get("XDG_DATA_HOME", os.path.expanduser("~/.local/share")),
        "sounds/effects",
        file,
    )
    if os.path.exists(sound_path):
        try:
            subprocess.Popen(
                ["ffplay", "-nodisp", "-autoexit", sound_path],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
        except Exception as e:
            logging.error(f"Failed to play sound {sound_path}: {e}")
            pass
    else:
        logging.warning(f"Sound file not found: {sound_path}")


class Timer:
    def __init__(self, state_file: str) -> None:
        self.state_file: str = state_file
        self.state: TimerState = TimerState.READY
        self.end_time: datetime = datetime.min
        self.stopped_time: datetime = datetime.min
        self.start_time: datetime = datetime.min  # For stopwatch
        self.elapsed_time: timedelta = timedelta(0)  # For paused stopwatch
        if not os.path.exists(state_file):
            self.clear()
        self.load_state()

    def load_state(self) -> None:
        try:
            with open(self.state_file) as f:
                state = json.load(f)
                self.state = TimerState(state["state"])
                self.end_time = datetime.fromisoformat(state["end_time"])
                self.stopped_time = datetime.fromisoformat(state["stopped_time"])
                # Load start_time for stopwatch if it exists
                if "start_time" in state:
                    self.start_time = datetime.fromisoformat(state["start_time"])
                else:
                    self.start_time = datetime.min
                # Load elapsed_time for paused stopwatch
                if "elapsed_seconds" in state:
                    self.elapsed_time = timedelta(seconds=state["elapsed_seconds"])
                else:
                    self.elapsed_time = timedelta(0)
        except (FileNotFoundError, json.JSONDecodeError, KeyError, ValueError) as e:
            logging.warning(f"Failed to load state, resetting: {e}")
            self.clear()  # Reset to default if loading fails

    def save_state(self) -> None:
        state = {
            "state": self.state.value,
            "end_time": self.end_time.isoformat(),
            "stopped_time": self.stopped_time.isoformat(),
            "start_time": self.start_time.isoformat(),  # Save start_time for stopwatch
            "elapsed_seconds": self.elapsed_time.total_seconds(),  # Save elapsed_time for paused stopwatch
        }
        try:
            with open(self.state_file, "w") as f:
                json.dump(state, f)
        except OSError as e:
            logging.error(f"Failed to save state: {e}")

    def set(self, minutes: int) -> None:
        self._set_duration(timedelta(minutes=minutes))
        logging.info(f"Set timer for {minutes} minutes")

    def set_seconds(self, seconds: int) -> None:
        self._set_duration(timedelta(seconds=seconds))
        logging.info(f"Set timer for {seconds} seconds")

    def _set_duration(self, duration: timedelta) -> None:
        self.state = TimerState.COUNTING
        self.end_time = datetime.now() + duration
        self.stopped_time = datetime.min
        self.start_time = datetime.min  # Reset stopwatch time
        self.elapsed_time = timedelta(0)  # Reset elapsed time
        if duration.total_seconds() > 0:
            play_sound("nier_enter.mp3")
        else:  # Handle setting timer to 0 immediately
            self.state = TimerState.READY
            self.end_time = datetime.min
        self.save_state()

    def read(self) -> timedelta:
        if self.state == TimerState.COUNTING:
            remaining = self.end_time - datetime.now()
            if remaining.total_seconds() <= 0:
                # Check again after potentially saving to avoid race condition
                if (
                    self.state == TimerState.COUNTING
                ):  # Avoid double timeout if already processed
                    self.state = TimerState.TIMEOUT
                    self.save_state()
                    play_sound("nier_back.mp3")
                    logging.info("Timeout")
                return timedelta(0)
            return remaining
        elif self.state == TimerState.STOPPED:
            # Ensure stopped time is valid before calculation
            if self.stopped_time > datetime.min and self.end_time > self.stopped_time:
                return self.end_time - self.stopped_time
            else:
                # If stopped_time is invalid, treat as 0 remaining
                logging.warning("Invalid state detected in read() for STOPPED timer.")
                return timedelta(0)
        elif self.state == TimerState.STOPWATCH:
            # Return elapsed time since start
            if self.start_time > datetime.min:
                elapsed = datetime.now() - self.start_time
                return elapsed
            else:
                logging.warning("Invalid state detected in read() for STOPWATCH.")
                return timedelta(0)
        elif self.state == TimerState.STOPWATCH_PAUSED:
            # Return the stored elapsed time
            return self.elapsed_time

        return timedelta(0)

    def print_time(self) -> dict:
        self.load_state()  # Ensure we have the latest state before printing
        if self.state == TimerState.READY:
            return {
                "text": f"<span font='{FONT_SIZE}' rise='-2000'>󰔛</span>",
                "tooltip": "Timer: Ready",
                "class": "ready",
            }
        elif self.state == TimerState.TIMEOUT:
            return {
                "text": f"<span font='{FONT_SIZE}' rise='-2000'>󰔛</span>",
                "tooltip": "Timer: Timeout!",
                "class": "timeout",
            }
        elif self.state == TimerState.COUNTING or self.state == TimerState.STOPPED:
            remaining_time = self.read()
            # Recalculate remaining time if state is stopped
            if self.state == TimerState.STOPPED:
                if (
                    self.stopped_time > datetime.min
                    and self.end_time > self.stopped_time
                ):
                    remaining_time = self.end_time - self.stopped_time
                else:
                    remaining_time = timedelta(0)

            total_seconds = int(remaining_time.total_seconds())
            minutes, seconds = divmod(max(0, total_seconds), 60)  # Ensure non-negative

            icon = "󰔟" if self.state == TimerState.COUNTING else "󰏤"
            css_class = "active" if self.state == TimerState.COUNTING else "stopped"
            tooltip_state = (
                "Counting" if self.state == TimerState.COUNTING else "Stopped"
            )

            return {
                "text": f"<span font='{FONT_SIZE}' rise='-2000'>{icon}</span> {minutes}:{str(seconds).zfill(2)} ",
                "class": css_class,
                "tooltip": f"Timer: {tooltip_state}\nEnds at: {self.end_time.strftime('%H:%M:%S') if self.end_time > datetime.min else 'N/A'}",
            }
        elif (
            self.state == TimerState.STOPWATCH
            or self.state == TimerState.STOPWATCH_PAUSED
        ):
            # Calculate elapsed time for stopwatch
            elapsed_time = self.read()
            total_seconds = int(elapsed_time.total_seconds())
            minutes, seconds = divmod(total_seconds, 60)
            hours, minutes = divmod(minutes, 60)

            # Display format depends on elapsed time
            if hours > 0:
                time_display = (
                    f"{hours}:{str(minutes).zfill(2)}:{str(seconds).zfill(2)}"
                )
            else:
                time_display = f"{minutes}:{str(seconds).zfill(2)}"

            # Use different icon and class for active vs paused stopwatch
            is_active = self.state == TimerState.STOPWATCH
            icon = "⏱️" if is_active else "󰏤"
            css_class = "stopwatch" if is_active else "stopwatch-paused"
            tooltip_state = "Running" if is_active else "Paused"
            tooltip_text = f"Stopwatch: {tooltip_state}"

            if is_active and self.start_time > datetime.min:
                tooltip_text += f"\nStarted at: {self.start_time.strftime('%H:%M:%S')}"

            return {
                "text": f"<span font='{FONT_SIZE}' rise='-2000'>{icon}</span> {time_display}",
                "class": css_class,
                "tooltip": tooltip_text,
            }
        else:  # Should not happen with Enum, but good practice
            return {
                "text": f"<span font='{FONT_SIZE}' rise='-2000'>?</span>",
                "tooltip": "Timer: Unknown State",
                "class": "unknown",
            }

    def toggle(self) -> None:
        self.load_state()  # Ensure current state
        if self.state == TimerState.COUNTING:
            self.state = TimerState.STOPPED
            self.stopped_time = datetime.now()
            logging.info("Stopped timer")
            play_sound("nier_cancel.mp3")
        elif self.state == TimerState.STOPPED:
            self.state = TimerState.COUNTING
            # Check if stopped_time is valid
            if self.stopped_time > datetime.min and self.end_time > self.stopped_time:
                duration_remaining = self.end_time - self.stopped_time
                self.end_time = datetime.now() + duration_remaining
            else:
                # If state was inconsistent, maybe just restart from 0 or keep original end time?
                # Let's restart from original end_time - now(), assuming it was just paused briefly.
                # Or maybe better, clear the timer if state is inconsistent?
                # For simplicity, let's just recalculate based on original end_time if possible
                # If end_time itself is invalid, we should probably clear.
                if self.end_time > datetime.now():
                    # This path implies it was stopped but state wasn't saved correctly.
                    # Keep original end_time.
                    pass  # end_time remains the same
                else:
                    # Timer already expired or state is very wrong, clear it.
                    logging.warning(
                        "Inconsistent STOPPED state during toggle, clearing timer."
                    )
                    self.clear()
                    return  # Exit toggle after clearing
            self.stopped_time = datetime.min  # Reset stopped time
            logging.info("Started timer")
            play_sound("nier_select.mp3")
        elif self.state == TimerState.STOPWATCH:
            # Pause the stopwatch
            self.state = TimerState.STOPWATCH_PAUSED
            # Calculate elapsed time up to now and store it
            if self.start_time > datetime.min:
                self.elapsed_time = datetime.now() - self.start_time
            self.start_time = datetime.min  # Clear start time
            logging.info("Paused stopwatch")
            play_sound("nier_cancel.mp3")
        elif self.state == TimerState.STOPWATCH_PAUSED:
            # Resume the stopwatch
            self.state = TimerState.STOPWATCH
            # Set start time to now minus the elapsed time
            self.start_time = datetime.now() - self.elapsed_time
            logging.info("Resumed stopwatch")
            play_sound("nier_select.mp3")
        elif self.state == TimerState.TIMEOUT:
            self.clear()  # Clear on toggle if timed out
            return
        else:  # READY state
            # Optional: maybe start a default timer? For now, do nothing.
            logging.info("Toggle attempted on READY timer.")
            return
        self.save_state()

    def start_stopwatch(self) -> None:
        self.state = TimerState.STOPWATCH
        self.start_time = datetime.now()
        self.end_time = datetime.min
        self.stopped_time = datetime.min
        self.elapsed_time = timedelta(0)
        self.save_state()
        logging.info("Started stopwatch")
        play_sound("nier_enter.mp3")

    def clear(self) -> None:
        play_sound("nier_cancel.mp3")
        self.state = TimerState.READY
        self.end_time = datetime.min
        self.stopped_time = datetime.min
        self.start_time = datetime.min
        self.elapsed_time = timedelta(0)
        self.save_state()
        logging.info("Cleared timer")


def run_cmd(cmd: str) -> str:
    try:
        # Use list format for better security and handling of spaces
        result = subprocess.run(
            cmd, shell=True, check=True, capture_output=True, text=True
        )
        return result.stdout.strip()
    except subprocess.CalledProcessError as e:
        logging.error(f"Command failed: {cmd}\nError: {e}")
        raise  # Re-raise the exception if the caller needs to handle it
    except FileNotFoundError:
        logging.error(f"Command not found: {ZENITY}")
        raise  # Re-raise


def main():
    parser = argparse.ArgumentParser(description="Waybar Timer Module")
    parser.add_argument(
        "-r",
        "--read",
        action="store_true",
        help="Read remaining time and print Waybar JSON output",
    )
    parser.add_argument(
        "-m",
        "--minute",
        nargs="?",
        const=None,  # Allows -m without value
        default=False,  # Differentiates between not provided and provided without value
        type=int,  # Check if value provided is int
        help="Set timer in minutes. No value opens GUI prompt.",
    )
    parser.add_argument(
        "-s",
        "--second",
        type=int,
        help="Set the timer in seconds (via CLI only)",
    )
    parser.add_argument(
        "-t",
        "--toggle",
        action="store_true",
        help="Toggle timer state (Counting <-> Stopped)",
    )
    parser.add_argument(
        "-c", "--clear", action="store_true", help="Clear/reset the timer"
    )
    parser.add_argument(
        "-w", "--watch", action="store_true", help="Start stopwatch mode"
    )
    args = parser.parse_args()

    timer = Timer(TIMER_FILE)

    if args.read:
        print(json.dumps(timer.print_time()))
    elif args.watch:
        timer.start_stopwatch()
    elif args.minute is not False:
        if args.minute is None:  # -m without value, use GUI
            try:
                timer_target_str = run_cmd(
                    f'{ZENITY} --scale --title "Set timer" --text "Set timer duration (minutes):" --min-value=0 --max-value=600 --step=1 --value={DEFAULT_MINUTE}'
                )
                if timer_target_str:  # Check if user provided input (didn't cancel)
                    timer_target = int(timer_target_str)
                    timer.set(timer_target)
                else:
                    logging.info("Timer setting cancelled via GUI.")
                    # print("{}")
            except (subprocess.CalledProcessError, FileNotFoundError, ValueError) as e:
                logging.error(f"Failed to get timer value via Zenity: {e}")
                print(
                    json.dumps(
                        {
                            "text": "Error",
                            "tooltip": f"Failed to run zenity: {e}",
                            "class": "error",
                        }
                    )
                )
                # Optionally exit with error code? return 1
            except Exception as e:
                logging.error(f"An unexpected error occurred with Zenity: {e}")
                print(
                    json.dumps(
                        {
                            "text": "Error",
                            "tooltip": f"Unexpected error: {e}",
                            "class": "error",
                        }
                    )
                )

        else:  # -m with a value
            try:
                timer_minutes = int(args.minute)
                if timer_minutes >= 0:
                    timer.set(timer_minutes)
                    print(
                        f"Timer set for {timer_minutes} minutes."
                    )  # User feedback for CLI set
                else:
                    print("Error: Minute value must be non-negative.")
                    logging.warning("Attempted to set negative minutes.")
            except ValueError:
                print("Error: Invalid minute value provided.")
                logging.error(f"Invalid minute value: {args.minute}")

    elif args.second is not None:  # -s used with a value
        try:
            timer_seconds = int(args.second)
            if timer_seconds >= 0:
                timer.set_seconds(timer_seconds)
                print(
                    f"Timer set for {timer_seconds} seconds."
                )  # User feedback for CLI set
            else:
                print("Error: Second value must be non-negative.")
                logging.warning("Attempted to set negative seconds.")
        except ValueError:
            print("Error: Invalid second value provided.")
            logging.error(f"Invalid second value: {args.second}")

    elif args.toggle:
        timer.toggle()
    elif args.clear:
        timer.clear()


if __name__ == "__main__":
    main()
