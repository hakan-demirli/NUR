"""End to end checks that the interface really draws in place.

A real pseudo terminal is driven and the bytes it receives are replayed
through a small terminal model, so the assertions are about what a person
would see rather than about which escape sequences were emitted.
"""

from __future__ import annotations

import contextlib
import fcntl
import os
import pty
import re
import select
import struct
import sys
import termios
import time
from collections.abc import Callable
from datetime import UTC, datetime
from pathlib import Path

import pytest

from tasktui.task.duckdb_store import DuckdbStore
from tasktui.task.model import Description, ProjectPath

# Forking from the test runner is exactly what these tests are for.
pytestmark = pytest.mark.filterwarnings("ignore:This process .* is multi-threaded")

ROWS, COLS = 20, 72
PROMPT = ["emre@box ~/state/tt", "> tt"]
POLL_SECONDS = 0.02
PATIENCE_SECONDS = 15.0
ANSI_OSC = re.compile(r"\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)")
ANSI_CSI = re.compile(r"\x1b\[([0-9;?]*)([A-Za-z])")


class Terminal:
    """Just enough of a terminal to see where text ends up."""

    def __init__(self, rows: int, cols: int) -> None:
        self.rows, self.cols = rows, cols
        self.grid = [[" "] * cols for _ in range(rows)]
        self.row = self.col = 0
        self.above: list[str] = []

    def feed(self, data: str) -> Terminal:
        index = 0
        while index < len(data):
            character = data[index]
            if character == "\x1b":
                index += self._escape(data[index:])
                continue
            if character == "\n":
                self._newline()
            elif character == "\r":
                self.col = 0
            elif character >= " " and self.col < self.cols:
                self.grid[self.row][self.col] = character
                self.col += 1
            index += 1
        return self

    def _escape(self, rest: str) -> int:
        osc = ANSI_OSC.match(rest)
        if osc:
            return osc.end()
        csi = ANSI_CSI.match(rest)
        if not csi:
            return 1
        self._csi(
            [int(p) for p in csi.group(1).split(";") if p.isdigit()], csi.group(2)
        )
        return csi.end()

    def _csi(self, numbers: list[int], final: str) -> None:
        count = numbers[0] if numbers else 1
        if final == "A":
            self.row = max(0, self.row - count)
        elif final == "B":
            self.row = min(self.rows - 1, self.row + count)
        elif final == "J" and (not numbers or numbers[0] == 0):
            self._erase_line_from(self.col)
            for row in range(self.row + 1, self.rows):
                self.grid[row] = [" "] * self.cols
        elif final == "K":
            if numbers and numbers[0] == 2:
                self.grid[self.row] = [" "] * self.cols
            elif not numbers or numbers[0] == 0:
                self._erase_line_from(self.col)

    def _erase_line_from(self, start: int) -> None:
        for column in range(start, self.cols):
            self.grid[self.row][column] = " "

    def _newline(self) -> None:
        self.row += 1
        if self.row < self.rows:
            return
        self.above.append("".join(self.grid[0]).rstrip())
        self.grid.pop(0)
        self.grid.append([" "] * self.cols)
        self.row -= 1

    def visible(self) -> list[str]:
        lines = self.above + ["".join(row).rstrip() for row in self.grid]
        while lines and not lines[-1]:
            lines.pop()
        return lines


class Session:
    """A tt process attached to a pseudo terminal."""

    def __init__(self, database: Path, *arguments: str) -> None:
        self.pid, self.fd = pty.fork()
        if self.pid == 0:
            try:
                os.environ["PYTHONPATH"] = os.pathsep.join(sys.path)
                os.environ["TERM"] = "xterm-256color"
                os.environ.pop("COLUMNS", None)
                os.environ.pop("LINES", None)
                os.write(1, ("\n".join(PROMPT) + "\n").encode())
                os.execv(
                    sys.executable,
                    [
                        sys.executable,
                        "-m",
                        "tasktui",
                        "--db",
                        str(database),
                        *arguments,
                    ],
                )
            # Nothing may escape a forked child, or pytest would run twice.
            except BaseException:  # pragma: no cover
                os._exit(127)
        fcntl.ioctl(self.fd, termios.TIOCSWINSZ, struct.pack("HHHH", ROWS, COLS, 0, 0))
        self.received = ""
        self.finished = False

    def _pump(self, seconds: float) -> bool:
        """Absorb whatever is waiting, reporting whether the process is gone."""
        if not select.select([self.fd], [], [], seconds)[0]:
            return False
        try:
            data = os.read(self.fd, 65536)
        except OSError:
            self.finished = True
            return True
        if not data:
            self.finished = True
            return True
        self.received += data.decode("utf-8", "replace")
        return False

    def press(self, keys: str) -> None:
        os.write(self.fd, keys.encode())

    def pump(self, seconds: float) -> None:
        """Absorb output for a fixed spell, for asserting nothing happened."""
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline and not self.finished:
            self._pump(POLL_SECONDS)

    def until(self, expected: Callable[[list[str]], bool], what: str) -> list[str]:
        """Read until the screen looks right, rather than for a fixed time.

        Waiting on the outcome instead of the clock keeps these tests quick on
        a fast machine and reliable on a slow one.
        """
        deadline = time.monotonic() + PATIENCE_SECONDS
        while True:
            screen = self.screen()
            if expected(screen):
                return screen
            if self.finished or time.monotonic() > deadline:
                shown = "\n".join(screen)
                raise AssertionError(f"timed out waiting for {what}, saw:\n{shown}")
            self._pump(POLL_SECONDS)

    def until_exit(self) -> list[str]:
        """Read until the process lets go of the terminal."""
        deadline = time.monotonic() + PATIENCE_SECONDS
        while not self.finished and time.monotonic() < deadline:
            self._pump(POLL_SECONDS)
        assert self.finished, "the process did not exit"
        return self.screen()

    def screen(self) -> list[str]:
        return Terminal(ROWS, COLS).feed(self.received).visible()

    def close(self) -> None:
        with contextlib.suppress(OSError):
            os.close(self.fd)
        with contextlib.suppress(ChildProcessError):
            os.waitpid(self.pid, 0)


@pytest.fixture
def seeded(tmp_path: Path) -> Path:
    database = tmp_path / "tasks.duckdb"
    store = DuckdbStore.open(database)
    now = datetime(2026, 1, 1, tzinfo=UTC)
    store.add(Description.parse("read the docs"), ProjectPath.parse("home"), now)
    store.add(Description.parse("write the code"), ProjectPath.parse("home"), now)
    store.add(Description.parse("ssh into the box"), None, now)
    store.close()
    return database


def shows(text: str) -> Callable[[list[str]], bool]:
    return lambda screen: any(text in line for line in screen)


def hides(text: str) -> Callable[[list[str]], bool]:
    return lambda screen: not any(text in line for line in screen)


def test_the_frame_is_drawn_below_the_prompt(seeded: Path) -> None:
    session = Session(seeded, "--project", "home")
    try:
        screen = session.until(shows("write the code"), "the task list")
    finally:
        session.close()
    assert screen[: len(PROMPT)] == PROMPT
    assert any("read the docs" in line for line in screen)


def test_quitting_erases_only_its_own_region(seeded: Path) -> None:
    """The scrollback above must be left exactly as it was found."""
    session = Session(seeded, "--project", "home")
    try:
        session.until(shows("read the docs"), "the task list")
        session.press("q")
        screen = session.until_exit()
    finally:
        session.close()
    assert screen == PROMPT


def test_interrupt_also_erases_its_own_region(seeded: Path) -> None:
    session = Session(seeded, "--project", "home")
    try:
        session.until(shows("read the docs"), "the task list")
        session.press("\x03")
        screen = session.until_exit()
    finally:
        session.close()
    assert screen == PROMPT


def test_a_task_added_through_the_interface_is_stored(seeded: Path) -> None:
    session = Session(seeded, "--project", "home")
    try:
        session.until(shows("read the docs"), "the task list")
        session.press("o")
        session.until(shows("add:"), "the add prompt")
        session.press("mow the lawn\r")
        session.until(shows("mow the lawn"), "the new task")
        session.press("q")
        session.until_exit()
    finally:
        session.close()

    store = DuckdbStore.open(seeded)
    descriptions = {task.description.text for task in store.snapshot().tasks}
    store.close()
    assert "mow the lawn" in descriptions


def test_a_project_can_be_created_from_the_interface(seeded: Path) -> None:
    """Projects exist only through tasks, so this is the only way in."""
    session = Session(seeded, "--no-project")
    try:
        session.until(shows("ssh into the box"), "the unassigned task")
        session.press("p")
        session.until(shows("project (blank for none):"), "the project prompt")
        session.press("work.ops\r")
        session.until(shows("moved to work.ops"), "confirmation")
        session.press("q")
        session.until_exit()
    finally:
        session.close()

    store = DuckdbStore.open(seeded)
    projects = {
        task.description.text: None if task.project is None else str(task.project)
        for task in store.snapshot().tasks
    }
    store.close()
    assert projects["ssh into the box"] == "work.ops"


def test_a_configuration_file_reaches_the_running_program(
    seeded: Path,
    tmp_path: Path,
) -> None:
    config = tmp_path / "config.toml"
    config.write_text('[keys.tasks]\nquit = ["x"]\n[theme]\ncursor = "white on blue"\n')
    session = Session(seeded, "--config", str(config), "--project", "home")
    try:
        screen = session.until(shows("read the docs"), "the task list")
        assert any("x quit" in line for line in screen), screen
        assert not any("q quit" in line for line in screen), screen

        session.press("q")
        session.pump(0.3)
        assert not session.finished, "q was rebound and should no longer quit"

        session.press("x")
        screen = session.until_exit()
    finally:
        session.close()
    assert screen == PROMPT


def test_the_frame_shrinks_back_without_leaving_debris(seeded: Path) -> None:
    """A frame that gets shorter must erase the rows it no longer uses."""
    session = Session(seeded, "--no-project")
    try:
        session.until(shows("ssh into the box"), "the unassigned task")
        session.press("o")
        session.until(shows("add:"), "the add prompt")
        session.press("\x1b")
        screen = session.until(hides("add:"), "the prompt to go")
        session.press("q")
        session.until_exit()
    finally:
        session.close()
    assert screen[: len(PROMPT)] == PROMPT
    assert any("ssh into the box" in line for line in screen)


def test_finished_work_stays_on_the_pane(seeded: Path) -> None:
    """Marking a task done keeps it as a record rather than dropping it."""
    session = Session(seeded, "--no-project")
    try:
        session.until(shows("ssh into the box"), "the unassigned task")
        session.press("d")
        screen = session.until(shows("done"), "the note saying so")
        session.press("q")
        session.until_exit()
    finally:
        session.close()
    assert any("ssh into the box" in line for line in screen)
