"""Reading and writing deadlines the way a person says them.

Deadlines are kept as instants in UTC but are talked about in local terms, so
"tomorrow" means the end of tomorrow where the person is sitting.  Everything
crosses that boundary here.
"""

from __future__ import annotations

import re
from datetime import UTC, date, datetime, time, timedelta, tzinfo
from typing import Final

END_OF_DAY: Final = time(23, 59)

SHORT_WEEKDAYS: Final = ("mon", "tue", "wed", "thu", "fri", "sat", "sun")
LONG_WEEKDAYS: Final = (
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
    "sunday",
)

UNITS: Final[dict[str, str]] = {"d": "days", "w": "weeks"}
CLEARING: Final = frozenset({"", "none", "clear", "never", "-"})

MINUTE_SECONDS: Final = 60
MINUTES_PER_HOUR: Final = 60
NOW_LABEL: Final = "now"

STAMPS: Final[tuple[tuple[str, bool], ...]] = (
    ("%Y-%m-%d %H:%M", False),
    ("%Y-%m-%dT%H:%M", False),
    ("%Y-%m-%d", True),
    ("%d.%m.%Y", True),
)

EXAMPLES: Final = "today, tomorrow, fri, 2026-09-01, 2026-09-01 17:00, +3d, +2w"

_OFFSET = re.compile(r"^\+(\d+)([dw])$")
_CLOCK = re.compile(r"^(\d{1,2}):(\d{2})$")


class InvalidDeadline(ValueError):
    """Raised when text cannot be read as a deadline."""


def _zoned(now: datetime) -> datetime:
    """The moment in the zone it carries, or the machine's if it carries none.

    Deadlines are said in local terms, so the zone attached to ``now`` is the
    one that decides which day "today" is.  Reaching for the machine's zone
    instead would make the answer depend on where the code happens to run.
    """
    return now if now.tzinfo is not None else now.astimezone()


def parse_due(text: str, now: datetime) -> datetime | None:
    """Read ``text`` as a deadline, relative to ``now``.

    Returns ``None`` when the text asks for the deadline to be taken away.
    A bare date means the end of that day rather than its first instant, since
    a deadline is a time to be finished by.

    Raises:
        InvalidDeadline: if the text says nothing recognisable.
    """
    wanted = text.strip().lower()
    if wanted in CLEARING:
        return None
    here = _zoned(now)
    zone = here.tzinfo
    assert zone is not None
    today = here.date()

    named = _named_day(wanted, today)
    if named is not None:
        return _combine(named, END_OF_DAY, zone)

    offset = _OFFSET.match(wanted)
    if offset is not None:
        span = timedelta(**{UNITS[offset.group(2)]: int(offset.group(1))})
        return _combine(today + span, END_OF_DAY, zone)

    clock = _CLOCK.match(wanted)
    if clock is not None:
        return _combine(today, _clock_time(clock), zone)

    return _stamp(text.strip(), zone)


def format_due(deadline: datetime | None, now: datetime) -> str:
    """A short rendering, leaning on how near the deadline is."""
    if deadline is None:
        return ""
    here = _zoned(now)
    when = deadline.astimezone(here.tzinfo)
    days = (when.date() - here.date()).days
    if days == 0:
        return when.strftime("%H:%M")
    if days == 1:
        return "tomorrow"
    if days == -1:
        return "yesterday"
    if -7 < days < 0:
        return f"{-days}d ago"
    if 0 < days < 7:
        return when.strftime("%a").lower()
    if when.year == here.year:
        return when.strftime("%-d %b").lower()
    return when.strftime("%Y-%m-%d")


def is_overdue(deadline: datetime | None, now: datetime) -> bool:
    """Whether a deadline has already gone by."""
    return deadline is not None and deadline < now


def format_remaining(deadline: datetime | None, now: datetime) -> str:
    """How long is left, counted in hours and minutes.

    Hours are the largest unit, so a deadline three days out reads as
    "72h 14min" rather than as a number of days.  Time already gone by is
    written with a leading minus.
    """
    if deadline is None:
        return ""
    span = deadline - now
    minutes = int(abs(span).total_seconds()) // MINUTE_SECONDS
    if minutes == 0:
        return NOW_LABEL
    hours, remainder = divmod(minutes, MINUTES_PER_HOUR)
    if not hours:
        counted = f"{remainder}min"
    elif not remainder:
        counted = f"{hours}h"
    else:
        counted = f"{hours}h {remainder}min"
    return f"-{counted}" if span < timedelta() else counted


def as_input(deadline: datetime | None, now: datetime) -> str:
    """The deadline written so that it can be edited and read back."""
    if deadline is None:
        return ""
    when = deadline.astimezone(_zoned(now).tzinfo)
    if when.time() == END_OF_DAY:
        return when.strftime("%Y-%m-%d")
    return when.strftime("%Y-%m-%d %H:%M")


def _named_day(wanted: str, today: date) -> date | None:
    if wanted == "today":
        return today
    if wanted == "tomorrow":
        return today + timedelta(days=1)
    if wanted == "yesterday":
        return today - timedelta(days=1)
    weekday = _weekday(wanted)
    if weekday is None:
        return None
    # A bare weekday means the next one, never the day it already is.
    ahead = (weekday - today.weekday()) % 7 or 7
    return today + timedelta(days=ahead)


def _weekday(wanted: str) -> int | None:
    for index, (short, long) in enumerate(
        zip(SHORT_WEEKDAYS, LONG_WEEKDAYS, strict=True)
    ):
        if wanted in {short, long}:
            return index
    return None


def _clock_time(match: re.Match[str]) -> time:
    hour, minute = int(match.group(1)), int(match.group(2))
    if hour > 23 or minute > 59:
        raise InvalidDeadline(f"{match.group(0)!r} is not a time of day")
    return time(hour, minute)


def _stamp(text: str, zone: tzinfo) -> datetime:
    for pattern, whole_day in STAMPS:
        try:
            parsed = datetime.strptime(text, pattern).replace(tzinfo=zone)
        except ValueError:
            continue
        moment = END_OF_DAY if whole_day else parsed.time()
        return _combine(parsed.date(), moment, zone)
    raise InvalidDeadline(f"cannot read {text!r} as a deadline; try {EXAMPLES}")


def _combine(day: date, moment: time, zone: tzinfo) -> datetime:
    return datetime.combine(day, moment, tzinfo=zone).astimezone(UTC)
