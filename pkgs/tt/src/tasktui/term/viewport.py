"""A drawing region that lives below the shell prompt.

Unlike a full screen interface this never switches to the alternate screen
buffer.  The frame is drawn where the cursor already is, redrawn in place, and
erased on the way out, so the scrollback above it is left exactly as it was.

Two rules keep that working.  The frame is never taller than the terminal,
because the cursor cannot be moved above the top of the screen to redraw it;
and the region is addressed relatively rather than by absolute row, so the
terminal scrolling underneath is harmless.
"""

from __future__ import annotations

import os
import select
import signal
import sys
import termios
import tty
from dataclasses import dataclass
from types import FrameType, TracebackType
from typing import Any, Final, Self

from rich.console import Console, RenderableType
from rich.live import Live

from tasktui.term.keys import KeyDecoder, KeyPress

ESCAPE_TIMEOUT_SECONDS: Final = 0.02
READ_SIZE: Final = 1024
PROMPT_RESERVE: Final = 1


@dataclass(frozen=True, slots=True)
class Resize:
    """The terminal changed size and the frame should be drawn again."""


Event = KeyPress | Resize


class NotATerminal(RuntimeError):
    """The interface was started without a terminal to draw on."""


class InlineViewport:
    """Owns the terminal for the lifetime of a ``with`` block.

    Terminal modes, the cursor and the drawn region are all restored on the
    way out, including when the block is left by an exception or a signal.
    """

    def __init__(self, console: Console) -> None:
        self._console = console
        self._live = Live(
            console=console,
            auto_refresh=False,
            transient=True,
            vertical_overflow="crop",
        )
        self._decoder = KeyDecoder()
        self._pending: list[KeyPress] = []
        self._input = sys.stdin.fileno()
        self._saved_modes: list[Any] | None = None
        self._wakeup: tuple[int, int] | None = None
        self._previous_winch: Any = None
        self._resized = False

    @property
    def height(self) -> int:
        """How many lines a frame may occupy."""
        return max(1, self._console.size.height - PROMPT_RESERVE)

    @property
    def width(self) -> int:
        """How many columns a frame may occupy."""
        return self._console.size.width

    def __enter__(self) -> Self:
        if not (self._console.is_terminal and os.isatty(self._input)):
            raise NotATerminal("tt needs an interactive terminal")
        self._enter_raw_mode()
        try:
            self._install_resize_handler()
            self._live.start()
        except BaseException:
            # Entering never completed, so __exit__ will not be called and
            # the terminal has to be handed back here.
            self._remove_resize_handler()
            self._leave_raw_mode()
            raise
        return self

    def __exit__(
        self,
        exception_type: type[BaseException] | None,
        exception: BaseException | None,
        traceback: TracebackType | None,
    ) -> None:
        self._live.stop()
        self._remove_resize_handler()
        self._leave_raw_mode()

    def render(self, frame: RenderableType) -> None:
        """Replace the drawn region with ``frame``."""
        self._live.update(frame, refresh=True)

    def next_event(self) -> Event | None:
        """Block until something happens, or return ``None`` at end of input."""
        while True:
            if self._pending:
                return self._pending.pop(0)
            if self._resized:
                self._resized = False
                return Resize()
            if not self._wait():
                self._pending.extend(self._decoder.flush())
                continue
            data = os.read(self._input, READ_SIZE)
            if not data:
                return None
            self._pending.extend(self._decoder.feed(data))

    def _wait(self) -> bool:
        """Wait for input, returning whether the terminal has bytes to read.

        A pending escape byte is given a short grace period to be followed by
        the rest of its sequence; if nothing arrives it was the escape key.
        """
        wakeup_read = self._wakeup[0] if self._wakeup else -1
        watched = [self._input] if wakeup_read < 0 else [self._input, wakeup_read]
        timeout = ESCAPE_TIMEOUT_SECONDS if self._decoder.pending else None
        readable, _, _ = select.select(watched, [], [], timeout)
        if wakeup_read in readable:
            os.read(wakeup_read, READ_SIZE)
        return self._input in readable

    def _enter_raw_mode(self) -> None:
        self._saved_modes = termios.tcgetattr(self._input)
        modes = termios.tcgetattr(self._input)
        # Signals stay off so that interrupt and suspend arrive as ordinary
        # key presses, and flow control stays off so that a stray ctrl-s
        # cannot wedge the terminal.
        modes[tty.LFLAG] &= ~(termios.ECHO | termios.ICANON | termios.ISIG)
        modes[tty.IFLAG] &= ~(termios.IXON | termios.ICRNL)
        modes[tty.CC][termios.VMIN] = 1
        modes[tty.CC][termios.VTIME] = 0
        termios.tcsetattr(self._input, termios.TCSADRAIN, modes)

    def _leave_raw_mode(self) -> None:
        if self._saved_modes is None:
            return
        termios.tcsetattr(self._input, termios.TCSADRAIN, self._saved_modes)
        self._saved_modes = None

    def _install_resize_handler(self) -> None:
        read_end, write_end = os.pipe()
        os.set_blocking(read_end, False)
        os.set_blocking(write_end, False)
        self._wakeup = (read_end, write_end)
        # The wakeup pipe is what breaks the blocking select; the handler only
        # has to record that the size changed.
        signal.set_wakeup_fd(write_end)
        self._previous_winch = signal.signal(signal.SIGWINCH, self._on_resize)

    def _remove_resize_handler(self) -> None:
        if self._wakeup is None:
            return
        signal.set_wakeup_fd(-1)
        signal.signal(signal.SIGWINCH, self._previous_winch)
        read_end, write_end = self._wakeup
        os.close(read_end)
        os.close(write_end)
        self._wakeup = None

    def _on_resize(self, _signal: int, _frame: FrameType | None) -> None:
        self._resized = True
