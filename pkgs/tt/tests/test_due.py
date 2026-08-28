from __future__ import annotations

from datetime import UTC, datetime, timedelta, timezone

import pytest

from tasktui.task.due import (
    END_OF_DAY,
    InvalidDeadline,
    as_input,
    format_due,
    format_remaining,
    is_overdue,
    parse_due,
)

# A Thursday at midday. Fixed offsets rather than named zones: the tests are
# about the offset carried by `now`, and a named zone would drag in the system
# time zone database and its daylight saving rules for no gain.
NOW = datetime(2026, 6, 4, 12, 0, tzinfo=UTC)
AHEAD = timezone(timedelta(hours=9))
BEHIND = timezone(timedelta(hours=-8))


def local(text: str, now: datetime = NOW) -> datetime:
    """Parse, and hand the answer back in the zone ``now`` was given in."""
    parsed = parse_due(text, now)
    assert parsed is not None
    return parsed.astimezone(now.tzinfo)


@pytest.mark.parametrize("text", ["", "  ", "none", "clear", "never", "-", "NONE"])
def test_emptiness_takes_the_deadline_away(text: str) -> None:
    assert parse_due(text, NOW) is None


def test_today_means_the_end_of_today() -> None:
    when = local("today")
    assert when.date() == NOW.date()
    assert when.time() == END_OF_DAY


def test_tomorrow_and_yesterday_move_a_day() -> None:
    today = NOW.date()
    assert local("tomorrow").date() == today + timedelta(days=1)
    assert local("yesterday").date() == today - timedelta(days=1)


@pytest.mark.parametrize(
    ("text", "ahead"),
    [("fri", 1), ("sat", 2), ("sun", 3), ("mon", 4), ("wed", 6), ("thu", 7)],
)
def test_a_weekday_means_the_next_one(text: str, ahead: int) -> None:
    """Thursday said on a Thursday means next Thursday, not today."""
    assert local(text).date() == NOW.date() + timedelta(days=ahead)


def test_weekdays_can_be_written_out_in_full() -> None:
    assert local("friday") == local("fri")


@pytest.mark.parametrize(
    ("text", "days"),
    [("+1d", 1), ("+3d", 3), ("+0d", 0), ("+1w", 7), ("+2w", 14)],
)
def test_an_offset_counts_forward(text: str, days: int) -> None:
    assert local(text).date() == NOW.date() + timedelta(days=days)


def test_a_bare_date_means_the_end_of_that_day() -> None:
    when = local("2026-09-01")
    assert (when.year, when.month, when.day) == (2026, 9, 1)
    assert when.time() == END_OF_DAY


@pytest.mark.parametrize("text", ["2026-09-01 17:30", "2026-09-01T17:30"])
def test_a_date_and_time_is_taken_literally(text: str) -> None:
    when = local(text)
    assert (when.hour, when.minute) == (17, 30)


def test_a_bare_time_means_today() -> None:
    when = local("17:30")
    assert when.date() == NOW.date()
    assert (when.hour, when.minute) == (17, 30)


def test_a_day_first_date_is_understood() -> None:
    when = local("01.09.2026")
    assert (when.year, when.month, when.day) == (2026, 9, 1)


@pytest.mark.parametrize(
    "text",
    ["soon", "next tuesday", "25:00", "12:99", "2026-13-01", "+3y", "+d", "tomorow"],
)
def test_nonsense_is_refused(text: str) -> None:
    with pytest.raises(InvalidDeadline):
        parse_due(text, NOW)


def test_the_refusal_suggests_what_would_work() -> None:
    with pytest.raises(InvalidDeadline, match="today"):
        parse_due("whenever", NOW)


def test_deadlines_are_stored_as_instants_in_utc() -> None:
    parsed = parse_due("today", NOW)
    assert parsed is not None
    assert parsed.tzinfo == UTC


@pytest.mark.parametrize("zone", [UTC, AHEAD, BEHIND])
@pytest.mark.parametrize("hour", [0, 12, 23])
def test_today_is_the_day_where_the_clock_is(zone: timezone, hour: int) -> None:
    """Late enough in the evening it is already another day elsewhere.

    Whose day counts is decided by the offset on ``now``, never by the machine
    the code is running on, or these answers would move between environments.
    """
    here = datetime(2026, 6, 4, hour, 30, tzinfo=zone)
    assert local("today", here).date() == here.date()
    assert local("today", here).time() == END_OF_DAY


@pytest.mark.parametrize("zone", [AHEAD, BEHIND])
def test_the_same_wall_clock_in_two_zones_means_two_instants(
    zone: timezone,
) -> None:
    here = datetime(2026, 6, 4, 12, 0, tzinfo=zone)
    assert parse_due("today", here) != parse_due("today", NOW)


@pytest.mark.parametrize(
    ("text", "shown"),
    [
        ("today", "23:59"),
        ("tomorrow", "tomorrow"),
        ("yesterday", "yesterday"),
        ("+3d", "sun"),
        ("2026-09-01", "1 sep"),
        ("2027-09-01", "2027-09-01"),
    ],
)
def test_deadlines_read_back_briefly(text: str, shown: str) -> None:
    assert format_due(parse_due(text, NOW), NOW) == shown


def test_a_few_days_past_is_counted_in_days() -> None:
    assert format_due(parse_due("+3d", NOW), NOW + timedelta(days=6)) == "3d ago"


def test_nothing_is_shown_for_no_deadline() -> None:
    assert format_due(None, NOW) == ""


@pytest.mark.parametrize(
    ("ahead", "shown"),
    [
        (timedelta(hours=55, minutes=56), "55h 56min"),
        (timedelta(hours=1, minutes=1), "1h 1min"),
        (timedelta(hours=2), "2h"),
        (timedelta(minutes=56), "56min"),
        (timedelta(minutes=1), "1min"),
        (timedelta(days=3), "72h"),
        (timedelta(seconds=59), "now"),
        (timedelta(0), "now"),
        (timedelta(minutes=-30), "-30min"),
        (timedelta(hours=-3, minutes=-20), "-3h 20min"),
    ],
)
def test_the_countdown_is_hours_and_minutes(ahead: timedelta, shown: str) -> None:
    """Hours stay the largest unit, so three days out reads as 72h."""
    assert format_remaining(NOW + ahead, NOW) == shown


def test_nothing_counts_down_without_a_deadline() -> None:
    assert format_remaining(None, NOW) == ""


def test_the_countdown_ignores_which_zone_it_is_asked_in() -> None:
    deadline = NOW + timedelta(hours=5)
    assert format_remaining(deadline, NOW.astimezone(AHEAD)) == "5h"
    assert format_remaining(deadline, NOW.astimezone(BEHIND)) == "5h"


def test_overdue_is_only_the_past() -> None:
    assert not is_overdue(None, NOW)
    assert not is_overdue(NOW + timedelta(minutes=1), NOW)
    assert is_overdue(NOW - timedelta(minutes=1), NOW)


@pytest.mark.parametrize("text", ["2026-09-01", "2026-09-01 17:30", "today", "+3d"])
def test_a_deadline_can_be_edited_and_read_back(text: str) -> None:
    """What is put in the field has to mean the same thing coming out."""
    once = parse_due(text, NOW)
    assert parse_due(as_input(once, NOW), NOW) == once


def test_a_whole_day_deadline_is_offered_back_as_a_bare_date() -> None:
    assert as_input(parse_due("2026-09-01", NOW), NOW) == "2026-09-01"


def test_a_timed_deadline_keeps_its_time_when_offered_back() -> None:
    assert as_input(parse_due("2026-09-01 17:30", NOW), NOW) == "2026-09-01 17:30"


def test_no_deadline_is_offered_back_as_an_empty_field() -> None:
    assert as_input(None, NOW) == ""
