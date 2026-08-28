from __future__ import annotations

import pytest

from tasktui.term.keys import (
    Char,
    Control,
    Key,
    KeyDecoder,
    KeyPress,
    key_name,
    parse_key,
)


def decode(*chunks: bytes) -> list[KeyPress]:
    decoder = KeyDecoder()
    presses: list[KeyPress] = []
    for chunk in chunks:
        presses.extend(decoder.feed(chunk))
    presses.extend(decoder.flush())
    return presses


@pytest.mark.parametrize(
    ("data", "expected"),
    [
        (b"a", [Char("a")]),
        (b"abc", [Char("a"), Char("b"), Char("c")]),
        (b" ", [Char(" ")]),
        (b"\r", [Key.ENTER]),
        (b"\n", [Key.ENTER]),
        (b"\t", [Key.TAB]),
        (b"\x7f", [Key.BACKSPACE]),
        (b"\x08", [Key.BACKSPACE]),
        (b"\x03", [Key.INTERRUPT]),
        (b"\x1b", [Key.ESCAPE]),
    ],
)
def test_single_bytes(data: bytes, expected: list[KeyPress]) -> None:
    assert decode(data) == expected


@pytest.mark.parametrize(
    ("data", "expected"),
    [
        (b"\x1b[A", Key.UP),
        (b"\x1b[B", Key.DOWN),
        (b"\x1b[C", Key.RIGHT),
        (b"\x1b[D", Key.LEFT),
        (b"\x1b[H", Key.HOME),
        (b"\x1b[F", Key.END),
        (b"\x1b[1~", Key.HOME),
        (b"\x1b[3~", Key.DELETE),
        (b"\x1b[5~", Key.PAGE_UP),
        (b"\x1b[6~", Key.PAGE_DOWN),
        (b"\x1bOA", Key.UP),
        (b"\x1bOH", Key.HOME),
    ],
)
def test_escape_sequences(data: bytes, expected: Key) -> None:
    assert decode(data) == [expected]


def test_sequences_split_across_reads_are_reassembled() -> None:
    assert decode(b"\x1b", b"[", b"A") == [Key.UP]
    assert decode(b"\x1b[", b"5~") == [Key.PAGE_UP]


def test_a_lone_escape_is_only_resolved_on_flush() -> None:
    decoder = KeyDecoder()
    assert decoder.feed(b"\x1b") == []
    assert decoder.pending
    assert decoder.flush() == [Key.ESCAPE]
    assert not decoder.pending


def test_multibyte_characters_decode() -> None:
    assert decode("ü".encode()) == [Char("ü")]
    assert decode("→".encode()) == [Char("→")]
    assert decode("🙂".encode()) == [Char("🙂")]


def test_multibyte_characters_split_across_reads() -> None:
    encoded = "ü".encode()
    assert decode(encoded[:1], encoded[1:]) == [Char("ü")]


def test_unrecognised_sequences_are_dropped_not_typed() -> None:
    """An unmapped key must never leak its name into a text field."""
    assert decode(b"\x1b[27;5u") == []
    assert decode(b"\x1b[200~") == []
    assert decode(b"\x1bZ") == []


def test_control_letters_arrive_as_themselves() -> None:
    assert decode(b"\x04") == [Control("d")]
    assert decode(b"\x01") == [Control("a")]
    assert decode(b"\x1a") == [Control("z")]


def test_control_letters_with_a_settled_meaning_keep_it() -> None:
    """Control with h has meant backspace since long before this program."""
    assert decode(b"\x08") == [Key.BACKSPACE]
    assert decode(b"\x09") == [Key.TAB]
    assert decode(b"\x0d") == [Key.ENTER]
    assert decode(b"\x03") == [Key.INTERRUPT]
    assert decode(b"\x15") == [Control("u")]


def test_control_letters_round_trip_through_their_name() -> None:
    assert parse_key("ctrl-d") == Control("d")
    assert key_name(Control("d")) == "ctrl-d"


def test_unprintable_characters_are_dropped() -> None:
    assert decode(b"\x00\x1c\x1f") == []


def test_invalid_utf8_is_dropped_without_raising() -> None:
    assert decode(b"\xff\xfe") == []


def test_text_around_an_escape_sequence_survives() -> None:
    assert decode(b"a\x1b[Bb") == [Char("a"), Key.DOWN, Char("b")]
