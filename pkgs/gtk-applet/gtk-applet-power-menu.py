#!/usr/bin/env python3
# ruff: noqa E402

import sys
import signal
import subprocess
import argparse

import gi

gi.require_version("Gtk", "3.0")
gi.require_version("AppIndicator3", "0.1")

from gi.repository import AppIndicator3, Gtk


class AppIndicatorExample:
    def __init__(self):
        self.app = "my-app-indicator"
        self.indicator = AppIndicator3.Indicator.new(
            id=self.app,
            icon_name="system-shutdown-symbolic",
            category=AppIndicator3.IndicatorCategory.APPLICATION_STATUS,
        )
        self.indicator.set_status(AppIndicator3.IndicatorStatus.ACTIVE)
        self.indicator.set_menu(self.create_menu())
        self.indicator.set_label("⏻", self.app)

    def create_menu(self):
        menu = Gtk.Menu()
        self.add_menu_item(menu, "Sleep", self.on_menu_item_click, "sleep")
        self.add_menu_item(menu, "Hibernate", self.on_menu_item_click, "hibernate")
        self.add_menu_item(menu, "Reboot", self.on_menu_item_click, "reboot")
        self.add_menu_item(menu, "Shutdown", self.on_menu_item_click, "shutdown")
        self.add_menu_item(menu, "Logout", self.on_menu_item_click, "logout")
        menu.show_all()
        return menu

    def add_menu_item(self, menu, label, callback, *args):
        item = Gtk.MenuItem(label=label)
        item.connect("activate", callback, *args)
        menu.append(item)

    def on_menu_item_click(self, source, action_name):
        script_path = sys.argv[0]
        subprocess.Popen([script_path, "--action", action_name])

    def run(self):
        Gtk.main()


def show_confirmation_dialog(action_name, confirmation_text, command):
    Gtk.init(None)
    dialog = Gtk.MessageDialog(
        parent=None,
        flags=0,
        message_type=Gtk.MessageType.QUESTION,
        buttons=Gtk.ButtonsType.YES_NO,
        text=f"Confirm {action_name.capitalize()}",
    )
    dialog.format_secondary_text(confirmation_text)

    settings = Gtk.Settings.get_default()
    settings.props.gtk_application_prefer_dark_theme = True

    response = dialog.run()
    dialog.destroy()

    if response == Gtk.ResponseType.YES:
        subprocess.run(command)


def main():
    actions = {
        "sleep": {
            "text": "Are you sure you want to sleep?",
            "command": ["systemctl", "suspend"],
        },
        "hibernate": {
            "text": "Are you sure you want to hibernate?",
            "command": ["systemctl", "hibernate"],
        },
        "reboot": {
            "text": "Are you sure you want to reboot?",
            "command": ["systemctl", "reboot"],
        },
        "shutdown": {
            "text": "Are you sure you want to shut down?",
            "command": ["systemctl", "poweroff"],
        },
        "logout": {
            "text": "Are you sure you want to logout?",
            "command": ["hyprctl", "dispatch", "exit"],
        },
    }

    if len(sys.argv) > 1:
        parser = argparse.ArgumentParser(description="Power menu confirmation tool.")
        parser.add_argument(
            "--action",
            type=str,
            required=True,
            choices=actions.keys(),
            help="The power action to perform.",
        )
        args = parser.parse_args()

        action_details = actions.get(args.action)
        if action_details:
            show_confirmation_dialog(
                args.action, action_details["text"], action_details["command"]
            )
    else:
        app = AppIndicatorExample()
        signal.signal(signal.SIGINT, signal.SIG_DFL)
        app.run()


if __name__ == "__main__":
    main()
