from __future__ import annotations

import io
import re
from dataclasses import replace
from datetime import UTC, datetime, timedelta

import pytest
from rich.color import Color
from rich.console import Console
from rich.style import Style

from tasktui.task.model import (
    AllProjects,
    Description,
    Done,
    Interval,
    NoProject,
    Pending,
    ProjectPath,
    Running,
    Snapshot,
    Task,
    TaskId,
    UnderProject,
)
from tasktui.term.keys import Char
from tasktui.ui.action import default_keymap, task_hints
from tasktui.ui.state import (
    Adding,
    AgendaScreen,
    Editing,
    Jumping,
    ListScreen,
    Normal,
    Note,
    Problem,
    Renaming,
    State,
    SummaryScreen,
    Tab,
)
from tasktui.ui.theme import DEFAULT_GLYPHS, DEFAULT_PALETTE, build_theme
from tasktui.ui.view import Budget, render

KEYMAP = default_keymap()

EPOCH = datetime(2026, 1, 1, tzinfo=UTC)
NOW = EPOCH + timedelta(hours=3)


def task(identifier: int, description: str, project: str | None = None) -> Task:
    return Task(
        id=TaskId(identifier),
        description=Description(description),
        project=None if project is None else ProjectPath.parse(project),
        state=Pending(),
        created_at=EPOCH,
    )


def snapshot_of(count: int, project: str | None = None) -> Snapshot:
    return Snapshot(
        tasks=tuple(
            task(index, f"task number {index}", project)
            for index in range(1, count + 1)
        ),
    )


ANSI = re.compile(r"\x1b(?:[@-Z\\-_]|\[[0-?]*[ -/]*[@-~])")


def drawn(state: State, snapshot: Snapshot, height: int, width: int = 80) -> list[str]:
    """The lines a frame puts on the terminal, with styling removed."""
    console = Console(
        file=io.StringIO(),
        force_terminal=True,
        width=width,
        height=height,
        legacy_windows=False,
        theme=build_theme(),
    )
    with console.capture() as capture:
        console.print(render(state, snapshot, NOW, height, width, KEYMAP))
    rendered = capture.get().rstrip("\n").split("\n")
    return [ANSI.sub("", line).rstrip() for line in rendered]


def padded(state: State, snapshot: Snapshot, height: int, width: int = 80) -> list[str]:
    """The same lines, but with their trailing padding left intact."""
    console = Console(
        file=io.StringIO(),
        force_terminal=True,
        width=width,
        height=height,
        legacy_windows=False,
        theme=build_theme(),
    )
    with console.capture() as capture:
        console.print(render(state, snapshot, NOW, height, width, KEYMAP))
    rendered = capture.get().rstrip("\n").split("\n")
    return [ANSI.sub("", line) for line in rendered]


def style_for(
    text: str,
    state: State,
    snapshot: Snapshot,
    height: int = 10,
    width: int = 80,
) -> Style:
    """The style rich settles on for the piece of a frame holding ``text``.

    Escape codes are no good to compare against: how a colour is written
    depends on how much colour the terminal running the tests owns up to, and
    that differs between one machine and the next.  The style rich resolves is
    the same either way.
    """
    console = Console(
        file=io.StringIO(),
        force_terminal=True,
        width=width,
        height=height,
        legacy_windows=False,
        theme=build_theme(),
    )
    lines = console.render_lines(
        render(state, snapshot, NOW, height, width, KEYMAP),
        pad=False,
    )
    return next(
        piece.style
        for line in lines
        for piece in line
        if text in piece.text and piece.style is not None
    )


def finished(identifier: int, description: str, project: str | None = None) -> Task:
    return replace(task(identifier, description, project), state=Done(EPOCH))


def a_list(cursor: int | None = 1) -> State:
    return State(
        agenda=AgendaScreen(None, Normal()),
        projects=ListScreen(
            scope=AllProjects(),
            cursor=None if cursor is None else TaskId(cursor),
            mode=Normal(),
        ),
        tab=Tab.PROJECTS,
    )


def a_summary() -> State:
    return State(
        agenda=AgendaScreen(None, Normal()),
        projects=SummaryScreen(cursor=AllProjects(), mode=Normal()),
        tab=Tab.PROJECTS,
    )


def an_agenda(cursor: int | None = 1) -> State:
    return State(
        agenda=AgendaScreen(None if cursor is None else TaskId(cursor), Normal()),
        projects=SummaryScreen(AllProjects(), Normal()),
        tab=Tab.AGENDA,
    )


@pytest.mark.parametrize("height", range(1, 25))
@pytest.mark.parametrize("count", [0, 1, 3, 40])
def test_a_frame_never_exceeds_its_line_budget(height: int, count: int) -> None:
    """A frame taller than the terminal could not be redrawn in place."""
    snapshot = snapshot_of(count)
    assert len(drawn(a_list(), snapshot, height)) <= height
    assert len(drawn(a_summary(), snapshot, height)) <= height


@pytest.mark.parametrize("width", [20, 34, 80, 200])
def test_no_line_is_wider_than_the_terminal(width: int) -> None:
    """A wrapped line would occupy two rows and desynchronise the redraw."""
    snapshot = Snapshot(
        tasks=(task(1, "a description far longer than any narrow terminal", "a.b.c"),),
    )
    for line in drawn(a_list(), snapshot, 10, width):
        assert len(line) <= width


@pytest.mark.parametrize("hints_wanted", [1, 2])
@pytest.mark.parametrize("height", range(1, 20))
def test_the_budget_never_starves_the_body(height: int, hints_wanted: int) -> None:
    budget = Budget.of(height, hints_wanted)
    parts = budget.title + budget.header + budget.body + budget.hints + budget.status
    assert budget.body >= 1
    assert parts <= max(height, 1)


@pytest.mark.parametrize("height", range(2, 20))
def test_the_status_line_survives_a_squeeze(height: int) -> None:
    """Whatever else goes, a person must still see what they are typing."""
    assert Budget.of(height, 2).status == 1


def test_the_reminders_are_the_first_thing_given_up() -> None:
    assert Budget.of(4, 2).hints == 0
    assert Budget.of(4, 2).status == 1
    assert Budget.of(4, 2).body >= 1


def shows(lines: list[str], number: int) -> bool:
    return any(re.search(rf"task number {number}\b", line) for line in lines)


@pytest.mark.parametrize("cursor", [1, 12, 25, 40])
def test_the_cursor_row_stays_visible_while_scrolling(cursor: int) -> None:
    assert shows(drawn(a_list(cursor=cursor), snapshot_of(40), 10), cursor)


def test_a_long_list_is_windowed_not_truncated_at_the_top() -> None:
    lines = drawn(a_list(cursor=40), snapshot_of(40), 10)
    assert shows(lines, 40)
    assert not shows(lines, 1)


def test_rows_reach_the_width_of_the_widest_line() -> None:
    """A selected row that stopped at its own text would look ragged."""
    lines = padded(a_summary(), snapshot_of(1, "pp"), 10, width=100)
    footer = len(lines[-1])
    assert footer > 0
    assert [len(line) for line in lines[1:]] == [footer] * (len(lines) - 1)


def test_a_wide_table_is_not_shrunk_to_the_hints() -> None:
    long = "a description considerably longer than the row of key reminders"
    snapshot = Snapshot(tasks=(task(1, long),))
    lines = padded(a_list(), snapshot, 10, width=120)
    assert len(lines[-1]) > max(len(hint.text) for hint in task_hints(KEYMAP))
    assert all(len(line) == len(lines[-1]) for line in lines[1:])


def test_the_frame_still_fits_a_terminal_narrower_than_the_hints() -> None:
    lines = padded(a_summary(), snapshot_of(1, "pp"), 10, width=30)
    assert all(len(line) <= 30 for line in lines)


def test_the_title_reports_the_position() -> None:
    lines = drawn(a_list(cursor=12), snapshot_of(40), 12)
    assert "12/40" in lines[0]


def test_an_empty_list_says_so() -> None:
    assert any(
        "no tasks here" in line for line in drawn(a_list(None), snapshot_of(0), 8)
    )


def test_the_agenda_counts_down_to_each_deadline() -> None:
    snapshot = Snapshot(
        tasks=(
            replace(task(1, "soon"), due=NOW + timedelta(hours=55, minutes=56)),
            replace(task(2, "late"), due=NOW - timedelta(hours=3, minutes=20)),
        ),
    )
    lines = drawn(an_agenda(1), snapshot, 10)
    assert any("left" in line for line in lines)
    assert any("55h 56min" in line for line in lines)
    assert any("-3h 20min" in line for line in lines)


def test_only_the_agenda_counts_down() -> None:
    """The other panes are not about deadlines, so they are not widened."""
    snapshot = Snapshot(
        tasks=(replace(task(1, "soon"), due=NOW + timedelta(hours=9)),),
    )
    assert not any("left" in line for line in drawn(a_list(), snapshot, 10))
    assert any("left" in line for line in drawn(an_agenda(1), snapshot, 10))


FADED = Color.parse(DEFAULT_PALETTE.outline)


def test_each_pane_stands_on_its_own_ground() -> None:
    """The ground is what says which pane is in front before it is read."""
    snapshot = Snapshot(tasks=(replace(task(1, "one", "home"), due=NOW),))
    grounds = {
        name: style_for(text, state, snapshot).bgcolor
        for name, state, text in (
            ("agenda", an_agenda(None), "one"),
            ("tree", a_summary(), "home"),
            ("list", a_list(None), "one"),
        )
    }
    assert len(set(grounds.values())) == len(grounds), grounds
    assert grounds["agenda"] == Color.parse(DEFAULT_PALETTE.ground_agenda)
    assert grounds["tree"] == Color.parse(DEFAULT_PALETTE.ground_projects)
    assert grounds["list"] == Color.parse(DEFAULT_PALETTE.ground_tasks)


def test_the_ground_reaches_the_whole_width_of_the_frame() -> None:
    """A ragged edge would read as a stripe rather than as a pane."""
    snapshot = Snapshot(tasks=(task(1, "one", "home"),))
    console = Console(
        file=io.StringIO(),
        force_terminal=True,
        width=80,
        height=10,
        legacy_windows=False,
        theme=build_theme(),
    )
    lines = console.render_lines(
        render(a_list(None), snapshot, NOW, 10, 80, KEYMAP),
        pad=False,
    )
    row = next(line for line in lines if any("one" in piece.text for piece in line))
    assert sum(len(piece.text) for piece in row) == 80
    last = row[-1]
    assert last.style is not None
    assert last.style.bgcolor == Color.parse(DEFAULT_PALETTE.ground_tasks)


def test_the_cursor_still_shows_over_the_ground() -> None:
    snapshot = Snapshot(tasks=(task(1, "one", "home"),))
    style = style_for("one", a_list(1), snapshot)
    assert style.bgcolor == Color.parse(DEFAULT_PALETTE.selection)


def test_the_row_identifier_is_not_shown() -> None:
    """It names a row in a database, and nothing anybody can act on here."""
    numbered = replace(task(1, "one"), id=TaskId(4242))
    lines = drawn(a_list(None), Snapshot(tasks=(numbered,)), 10)
    assert not any("4242" in line for line in lines)
    assert not any("id" in line.split() for line in lines)


def test_a_column_heading_is_not_the_colour_of_finished_work() -> None:
    """They were the same colour, so a heading read as a finished task."""
    snapshot = Snapshot(tasks=(finished(1, "all done"),))
    assert style_for("description", a_list(None), snapshot).color != FADED


def test_a_column_heading_stands_apart_from_the_rows_beneath_it() -> None:
    snapshot = Snapshot(tasks=(task(1, "still open"),))
    heading = style_for("description", a_list(None), snapshot)
    row = style_for("still open", a_list(None), snapshot)
    assert (heading.color, heading.bold) != (row.color, row.bold)


def test_every_pane_heads_its_columns_the_same_way() -> None:
    snapshot = Snapshot(tasks=(replace(task(1, "one", "home"), due=NOW),))
    headings = {
        style_for(text, state, snapshot).without_color
        for state, text in (
            (an_agenda(None), "left"),
            (a_summary(), "open"),
            (a_list(None), "description"),
        )
    }
    assert len(headings) == 1


def test_finished_work_is_faded_out() -> None:
    snapshot = Snapshot(tasks=(task(1, "still open"), finished(2, "all done")))
    assert style_for("all done", a_list(), snapshot).color == FADED


def test_finished_work_is_marked_as_well_as_faded() -> None:
    """Colour alone would say nothing on a terminal that has none."""
    snapshot = Snapshot(tasks=(finished(1, "all done"),))
    row = next(line for line in drawn(a_list(), snapshot, 10) if "all done" in line)
    assert DEFAULT_GLYPHS.done in row


def test_a_finished_task_is_not_shouted_at_for_being_late() -> None:
    """Its deadline has stopped mattering, so it must not read as overdue."""
    late = replace(finished(1, "late but done"), due=NOW - timedelta(days=1))
    snapshot = Snapshot(tasks=(late,))
    assert style_for("late but done", a_list(), snapshot).color == FADED


def test_the_cursor_can_rest_on_finished_work() -> None:
    """The fade and the cursor have to be able to land on the same row."""
    snapshot = Snapshot(tasks=(finished(1, "all done"),))
    style = style_for("all done", a_list(1), snapshot)
    assert style.color == FADED
    assert style.bgcolor == Color.parse(DEFAULT_PALETTE.selection)


def test_work_still_to_do_is_not_faded() -> None:
    snapshot = Snapshot(tasks=(task(1, "still open"),))
    assert style_for("still open", a_list(), snapshot).color != FADED


def test_the_running_task_is_marked() -> None:
    snapshot = Snapshot(
        tasks=snapshot_of(3).tasks,
        running=(Running(task_id=TaskId(2), since=EPOCH),),
        intervals=(Interval(task_id=TaskId(2), started_at=EPOCH, stopped_at=None),),
    )
    lines = drawn(a_list(cursor=2), snapshot, 10)
    marked = next(line for line in lines if "task number 2" in line)
    assert "*" in marked
    assert "3h00" in marked


def test_typing_shows_the_buffer() -> None:
    state = a_list().showing(ListScreen(AllProjects(), TaskId(1), Adding("buy milk")))
    assert any("add: buy milk" in line for line in drawn(state, snapshot_of(3), 10))


def test_editing_shows_the_buffer() -> None:
    state = a_list().showing(
        ListScreen(AllProjects(), TaskId(1), Editing(TaskId(1), "reword"))
    )
    assert any("edit: reword" in line for line in drawn(state, snapshot_of(3), 10))


def test_renaming_names_the_project_being_renamed() -> None:
    path = ProjectPath.parse("home.garden")
    state = a_summary().showing(
        SummaryScreen(UnderProject(path), Renaming(path, "shed"))
    )
    lines = drawn(state, snapshot_of(3, "home.garden"), 10)
    assert any("rename home.garden: shed" in line for line in lines)


def test_jumping_prompts_for_a_letter() -> None:
    state = a_summary().showing(SummaryScreen(AllProjects(), Jumping()))
    assert any("jump to project" in line for line in drawn(state, snapshot_of(3), 10))


def test_a_message_appears_on_the_status_line() -> None:
    """The reminders stay put; only the bottom line changes."""
    state = replace(a_list(), status=Problem("nope"))
    lines = drawn(state, snapshot_of(3), 10)
    assert lines[-1].startswith("nope")
    assert any("q quit" in line for line in lines)


def test_a_note_appears_on_the_status_line() -> None:
    state = replace(a_list(), status=Note("done"))
    assert drawn(state, snapshot_of(3), 10)[-1].startswith("done")


def test_typing_appears_on_the_status_line() -> None:
    state = a_list().showing(ListScreen(AllProjects(), TaskId(1), Adding("buy milk")))
    assert drawn(state, snapshot_of(3), 10)[-1].startswith("add: buy milk")


def test_a_half_typed_run_of_keys_appears_on_the_status_line() -> None:
    state = replace(a_list(), pending=(Char("g"),))
    assert drawn(state, snapshot_of(3), 10)[-1].startswith("g")


def test_a_half_typed_run_of_keys_does_not_resize_the_frame() -> None:
    """Pressing the first key of a run must not make the block jump."""
    settled = padded(a_list(), snapshot_of(3), 10)
    waiting = padded(replace(a_list(), pending=(Char("g"),)), snapshot_of(3), 10)
    assert [len(line) for line in settled] == [len(line) for line in waiting]


def test_a_message_does_not_resize_the_frame() -> None:
    settled = padded(a_list(), snapshot_of(3), 10)
    noted = padded(replace(a_list(), status=Note("done")), snapshot_of(3), 10)
    assert [len(line) for line in settled] == [len(line) for line in noted]


def tree_at(cursor: object) -> State:
    return State(
        agenda=AgendaScreen(None, Normal()),
        projects=SummaryScreen(cursor, Normal()),  # type: ignore[arg-type]
        tab=Tab.PROJECTS,
    )


def test_one_width_serves_every_pane_and_every_row() -> None:
    """The longest line anywhere sets the width, so nothing ever resizes it.

    A pane whose own content is short must still be drawn at the width the
    widest pane needs, or changing tab would make the block jump.
    """
    snapshot = Snapshot(
        tasks=(
            task(1, "short"),
            task(2, "a considerably longer description", "engineering.backend"),
            *(task(n, f"filler {n}", "bulk") for n in range(3, 30)),
        ),
    )
    everywhere = {
        len(line)
        for state in (
            an_agenda(None),
            tree_at(AllProjects()),
            tree_at(UnderProject(ProjectPath.parse("bulk"))),
            a_list(cursor=2),
            a_list(cursor=29),
        )
        for line in padded(state, snapshot, 8)
    }
    assert len(everywhere) == 1


def test_scrolling_a_long_list_does_not_resize_the_frame() -> None:
    """A long row scrolled out of sight still counts towards the width."""
    snapshot = Snapshot(
        tasks=(
            task(1, "a description far longer than any of the others here"),
            *(task(n, "short", None) for n in range(2, 40)),
        ),
    )
    top = padded(a_list(cursor=1), snapshot, 8)
    bottom = padded(a_list(cursor=39), snapshot, 8)
    assert {len(line) for line in top} == {len(line) for line in bottom}


def test_moving_between_tree_rows_does_not_resize_the_frame() -> None:
    """Some rows offer fewer keys; the block must not shrink to suit them."""
    snapshot = Snapshot(
        tasks=(task(1, "loose"), task(2, "filed", "pp")),
    )
    widths = {
        len(line)
        for cursor in (
            AllProjects(),
            NoProject(),
            UnderProject(ProjectPath.parse("pp")),
        )
        for line in padded(tree_at(cursor), snapshot, 12)
    }
    assert len(widths) == 1


def test_only_a_real_project_offers_renaming_and_deadlines() -> None:
    snapshot = Snapshot(tasks=(task(1, "filed", "pp"),))
    on_project = drawn(tree_at(UnderProject(ProjectPath.parse("pp"))), snapshot, 12)
    on_selection = drawn(tree_at(AllProjects()), snapshot, 12)
    for offered in ("D due", "r rename", "d forget"):
        assert any(offered in line for line in on_project), offered
        assert not any(offered in line for line in on_selection), offered
    assert any("o new" in line for line in on_selection)


def test_the_project_tree_is_indented_by_depth() -> None:
    snapshot = Snapshot(
        tasks=(task(1, "deep", "a.b.c"),),
    )
    lines = drawn(a_summary(), snapshot, 12)
    indents = {
        name: next(
            len(line) - len(line.lstrip())
            for line in lines
            if line.lstrip().startswith(f"{name} ")
        )
        for name in ("a", "b", "c")
    }
    assert indents["a"] < indents["b"] < indents["c"]
