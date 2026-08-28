"""Incremental decoding of terminal byte streams into key presses.

The decoder is deliberately free of I/O so that it can be exercised without a
terminal.  Callers push bytes in with :meth:`KeyDecoder.feed` and, when a read
times out while a lone escape byte is buffered, resolve the ambiguity with
:meth:`KeyDecoder.flush`.
"""

from __future__ import annotations

from dataclasses import dataclass
from enum import Enum, auto
from typing import Final


class Key(Enum):
    """A key press that does not correspond to a printable character."""

    UP = auto()
    DOWN = auto()
    LEFT = auto()
    RIGHT = auto()
    HOME = auto()
    END = auto()
    PAGE_UP = auto()
    PAGE_DOWN = auto()
    DELETE = auto()
    INSERT = auto()
    BACKSPACE = auto()
    ENTER = auto()
    TAB = auto()
    ESCAPE = auto()
    INTERRUPT = auto()


@dataclass(frozen=True, slots=True)
class Char:
    """A single printable character."""

    value: str


@dataclass(frozen=True, slots=True)
class Control:
    """A letter pressed with control held.

    Only letters without a settled meaning of their own arrive here.  Control
    with h, i, j, m and c have meant backspace, tab, return and interrupt
    since long before this program, so they are named for what they do rather
    than for how they are typed.
    """

    letter: str


KeyPress = Key | Char | Control

ESC: Final = 0x1B
# Control with a letter arrives as the byte that letter would be, less 96.
CONTROL_LETTERS: Final = range(0x01, 0x1B)
CONTROL_OFFSET: Final = 0x60
CSI_FINAL: Final = range(0x40, 0x7F)
CSI_PARAMETER: Final = range(0x30, 0x40)
CSI_INTERMEDIATE: Final = range(0x20, 0x30)

_CONTROL: Final[dict[int, Key]] = {
    0x03: Key.INTERRUPT,
    0x08: Key.BACKSPACE,
    0x09: Key.TAB,
    0x0A: Key.ENTER,
    0x0D: Key.ENTER,
    0x7F: Key.BACKSPACE,
}

_CSI_FINAL_KEYS: Final[dict[str, Key]] = {
    "A": Key.UP,
    "B": Key.DOWN,
    "C": Key.RIGHT,
    "D": Key.LEFT,
    "F": Key.END,
    "H": Key.HOME,
}

_CSI_TILDE_KEYS: Final[dict[str, Key]] = {
    "1": Key.HOME,
    "2": Key.INSERT,
    "3": Key.DELETE,
    "4": Key.END,
    "5": Key.PAGE_UP,
    "6": Key.PAGE_DOWN,
    "7": Key.HOME,
    "8": Key.END,
}

_SS3_KEYS: Final[dict[str, Key]] = {
    "A": Key.UP,
    "B": Key.DOWN,
    "C": Key.RIGHT,
    "D": Key.LEFT,
    "F": Key.END,
    "H": Key.HOME,
}


_NAMED_KEYS: Final[dict[str, Key]] = {
    "up": Key.UP,
    "down": Key.DOWN,
    "left": Key.LEFT,
    "right": Key.RIGHT,
    "home": Key.HOME,
    "end": Key.END,
    "pageup": Key.PAGE_UP,
    "pagedown": Key.PAGE_DOWN,
    "delete": Key.DELETE,
    "insert": Key.INSERT,
    "backspace": Key.BACKSPACE,
    "enter": Key.ENTER,
    "tab": Key.TAB,
    "esc": Key.ESCAPE,
    "ctrl-c": Key.INTERRUPT,
}

_KEY_NAMES: Final[dict[Key, str]] = {key: name for name, key in _NAMED_KEYS.items()}

SPACE_NAME: Final = "space"


class UnknownKey(ValueError):
    """Raised when text does not name a key that can be pressed."""


CONTROL_PREFIX: Final = "ctrl-"


def parse_key(text: str) -> KeyPress:
    """Read the name of a key, as written in a configuration file.

    A single printable character stands for itself, ``ctrl-`` and a letter is
    that letter held with control, and everything else has to be one of the
    names in :data:`_NAMED_KEYS`.

    Raises:
        UnknownKey: if the text names no key.
    """
    if text == SPACE_NAME:
        return Char(" ")
    named = _NAMED_KEYS.get(text.lower())
    if named is not None:
        return named
    held = text.lower().removeprefix(CONTROL_PREFIX)
    if held != text.lower() and len(held) == 1 and held.isalpha():
        return Control(held)
    if len(text) == 1 and text.isprintable() and not text.isspace():
        return Char(text)
    known = ", ".join(sorted([*_NAMED_KEYS, SPACE_NAME]))
    raise UnknownKey(
        f"unknown key {text!r}; use a single character, ctrl- and a letter, "
        f"or one of: {known}"
        " (for a sequence of keys, separate them with spaces, as in 'g n')"
    )


def parse_sequence(text: str) -> tuple[KeyPress, ...]:
    """Read a run of keys that has to be pressed in order.

    Keys are separated by spaces, so ``"g n"`` is g followed by n while
    ``"space"`` is the space bar.

    Raises:
        UnknownKey: if the text is empty or names no key.
    """
    tokens = text.split()
    if not tokens:
        raise UnknownKey("a binding needs at least one key")
    return tuple(parse_key(token) for token in tokens)


def sequence_name(sequence: tuple[KeyPress, ...]) -> str:
    """How a run of keys is written in a configuration file."""
    return " ".join(key_name(press) for press in sequence)


def sequence_label(sequence: tuple[KeyPress, ...]) -> str:
    """How a run of keys is shown to a person.

    Runs of plain characters read better closed up, the way they are typed.
    """
    names = [key_name(press) for press in sequence]
    if all(len(name) == 1 for name in names):
        return "".join(names)
    return " ".join(names)


def key_name(press: KeyPress) -> str:
    """How a key press is written in a configuration file."""
    match press:
        case Char(" "):
            return SPACE_NAME
        case Char(value):
            return value
        case Control(letter):
            return f"{CONTROL_PREFIX}{letter}"
        case _:
            return _KEY_NAMES[press]


def _utf8_length(lead: int) -> int:
    """Return the total byte length of the UTF-8 sequence starting with ``lead``."""
    if lead < 0x80:
        return 1
    if 0xC0 <= lead < 0xE0:
        return 2
    if 0xE0 <= lead < 0xF0:
        return 3
    if 0xF0 <= lead < 0xF8:
        return 4
    return 0


class KeyDecoder:
    """Turns a stream of terminal bytes into a stream of key presses.

    Bytes that do not form a recognised key are discarded rather than being
    surfaced as text, so that an unmapped escape sequence can never leak into a
    text field.
    """

    def __init__(self) -> None:
        self._buffer = bytearray()

    @property
    def pending(self) -> bool:
        """Whether undecoded bytes are buffered awaiting more input."""
        return bool(self._buffer)

    def feed(self, data: bytes) -> list[KeyPress]:
        """Decode everything unambiguously decodable from ``data`` and the buffer."""
        self._buffer.extend(data)
        presses: list[KeyPress] = []
        while self._buffer:
            press, consumed = self._decode()
            if consumed == 0:
                break
            del self._buffer[:consumed]
            if press is not None:
                presses.append(press)
        return presses

    def flush(self) -> list[KeyPress]:
        """Resolve a buffered lone escape byte as :attr:`Key.ESCAPE`.

        Anything else left in the buffer is an incomplete sequence that will
        never complete, and is discarded.
        """
        if not self._buffer:
            return []
        presses: list[KeyPress] = [Key.ESCAPE] if self._buffer[0] == ESC else []
        self._buffer.clear()
        return presses

    def _decode(self) -> tuple[KeyPress | None, int]:
        """Decode one key press, returning it and the bytes it consumed.

        A consumed count of zero means the buffer holds an incomplete prefix.
        """
        lead = self._buffer[0]
        if lead == ESC:
            return self._decode_escape()
        if lead in _CONTROL:
            return _CONTROL[lead], 1
        if CONTROL_LETTERS.start <= lead < CONTROL_LETTERS.stop:
            return Control(chr(lead + CONTROL_OFFSET)), 1
        if lead < 0x20:
            return None, 1
        return self._decode_text()

    def _decode_escape(self) -> tuple[KeyPress | None, int]:
        if len(self._buffer) < 2:
            return None, 0
        introducer = self._buffer[1]
        if introducer == ord("["):
            return self._decode_csi()
        if introducer == ord("O"):
            return self._decode_ss3()
        return None, 2

    def _decode_csi(self) -> tuple[KeyPress | None, int]:
        index = 2
        while index < len(self._buffer) and self._buffer[index] in CSI_PARAMETER:
            index += 1
        while index < len(self._buffer) and self._buffer[index] in CSI_INTERMEDIATE:
            index += 1
        if index >= len(self._buffer):
            return None, 0
        final = self._buffer[index]
        if final not in CSI_FINAL:
            return None, index + 1
        parameters = self._buffer[2:index].decode("ascii", "replace")
        consumed = index + 1
        if chr(final) == "~":
            return _CSI_TILDE_KEYS.get(parameters), consumed
        return _CSI_FINAL_KEYS.get(chr(final)), consumed

    def _decode_ss3(self) -> tuple[KeyPress | None, int]:
        if len(self._buffer) < 3:
            return None, 0
        return _SS3_KEYS.get(chr(self._buffer[2])), 3

    def _decode_text(self) -> tuple[KeyPress | None, int]:
        length = _utf8_length(self._buffer[0])
        if length == 0:
            return None, 1
        if len(self._buffer) < length:
            return None, 0
        try:
            text = bytes(self._buffer[:length]).decode("utf-8")
        except UnicodeDecodeError:
            return None, 1
        if not text.isprintable():
            return None, length
        return Char(text), length
