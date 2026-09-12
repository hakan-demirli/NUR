#!/usr/bin/env python3
import subprocess


def get_brightness():
    # Get brightness level using brightnessctl
    brightness_output = (
        subprocess.check_output(["brightnessctl", "get"]).decode("utf-8").strip()
    )
    return int(brightness_output)


def main():
    brightness_level = get_brightness()

    # Normalize brightness level to a float between 0 and 1
    normalized_brightness = brightness_level / 255.0

    # Call gtk-indicator-client with brightness level
    subprocess.run(
        [
            "gtk-indicator-client",
            "--status",
            "default",
            "--name",
            "brightness",
            "--progress",
            str(normalized_brightness),
        ]
    )


if __name__ == "__main__":
    main()
