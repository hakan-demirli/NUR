"""Turning state into a frame.

The frame is built to a line budget rather than trimmed afterwards.  A frame
taller than the terminal could not be redrawn in place, so the panes scroll
their own contents to stay inside it.
"""

from __future__ import annotations

from collections.abc import Callable
from dataclasses import dataclass, replace
from datetime import datetime, timedelta
from typing import Final

from rich.console import Console, ConsoleOptions, RenderableType, RenderResult
from rich.measure import Measurement
from rich.segment import Segment
from rich.table import Table
from rich.text import Text

from tasktui.task.due import EXAMPLES, format_due, format_remaining, is_overdue
from tasktui.task.model import (
    AllProjects,
    Done,
    NoProject,
    ProjectSummary,
    Snapshot,
    Task,
    TaskId,
    UnderProject,
    scope_label,
)
from tasktui.term.keys import sequence_label
from tasktui.ui import theme
from tasktui.ui.action import Hint, Keymap, project_hints, task_hints
from tasktui.ui.state import (
    TAB_LABELS,
    TAB_ORDER,
    Adding,
    AgendaScreen,
    Editing,
    Jumping,
    ListMode,
    ListScreen,
    Naming,
    Normal,
    Note,
    Problem,
    Renaming,
    Reprojecting,
    Scheduling,
    SchedulingProject,
    State,
    SummaryMode,
    SummaryScreen,
    Tab,
)
from tasktui.ui.theme import DEFAULT_GLYPHS, Glyphs, Look
from tasktui.ui.update import agenda_rows, list_rows, summary_rows

DEFAULT_LOOK: Final = Look()

HINT_LINES: Final = 2


@dataclass(frozen=True, slots=True)
class Budget:
    """How the available lines are shared between the parts of a frame.

    Parts are given up in order of how little they are missed: the reminders
    first, then the column names, then the tab strip.  The status line stays
    for as long as there is more than one line, because a person needs to see
    what they are typing.
    """

    title: int
    header: int
    body: int
    hints: int
    status: int

    @classmethod
    def of(cls, height: int, hints_wanted: int = 1) -> Budget:
        """Divide ``height`` lines, giving the body whatever is left."""
        status = 1 if height >= 2 else 0
        spare = height - status - 1
        title = 1 if spare >= 1 else 0
        header = 1 if spare - title >= 1 else 0
        hints = min(hints_wanted, max(0, spare - title - header))
        body = max(1, height - status - title - header - hints)
        return cls(
            title=title,
            header=header,
            body=body,
            hints=hints,
            status=status,
        )


def render(
    state: State,
    snapshot: Snapshot,
    now: datetime,
    height: int,
    width: int,
    keymap: Keymap,
    look: Look = DEFAULT_LOOK,
) -> RenderableType:
    """Build a frame of at most ``height`` lines."""
    glyphs = look.glyphs
    hints = _hints(state, keymap, glyphs)
    budget = Budget.of(height, hints.height(width))
    hints = replace(hints, limit=max(1, budget.hints))
    match state.screen:
        case AgendaScreen() as agenda:
            tasks = agenda_rows(snapshot)
            position = _place(agenda.cursor, [task.id for task in tasks])

            def rows_of_tasks(window: range) -> RenderableType:
                # The agenda is the pane about deadlines, so it is the one
                # that says how long is left on each.
                return _task_table(
                    tasks,
                    window,
                    agenda.cursor,
                    snapshot,
                    now,
                    budget,
                    glyphs,
                    projects=True,
                    countdown=True,
                )

            body, whole = _sized(rows_of_tasks, len(tasks), position, budget.body)
            title = _title(state, None, len(tasks), position, budget, glyphs)
            ground = theme.GROUND_AGENDA
        case SummaryScreen() as tree:
            summaries = summary_rows(snapshot, now)
            scopes = [row.scope for row in summaries]
            position = _place(tree.cursor, scopes)

            def rows_of_projects(window: range) -> RenderableType:
                return _summary_table(summaries, window, tree, now, budget, glyphs)

            body, whole = _sized(
                rows_of_projects, len(summaries), position, budget.body
            )
            title = _title(state, None, len(summaries), position, budget, glyphs)
            ground = theme.GROUND_PROJECTS
        case ListScreen() as listing:
            tasks = list_rows(snapshot, listing)
            position = _place(listing.cursor, [task.id for task in tasks])
            shows_project = isinstance(listing.scope, AllProjects)

            def rows_of_listed(window: range) -> RenderableType:
                return _task_table(
                    tasks,
                    window,
                    listing.cursor,
                    snapshot,
                    now,
                    budget,
                    glyphs,
                    projects=shows_project,
                )

            body, whole = _sized(rows_of_listed, len(tasks), position, budget.body)
            trail = _list_title(listing)
            title = _title(state, trail, len(tasks), position, budget, glyphs)
            ground = theme.GROUND_TASKS

    parts: list[RenderableType] = []
    if title is not None:
        parts.append(title)
    parts.append(body)
    if budget.hints:
        parts.append(hints)
    return Frame(
        tuple(parts),
        ground=ground,
        anchors=(whole, *_everything(snapshot, now, budget, glyphs, keymap)),
        trailer=_status(state, glyphs) if budget.status else None,
    )


def _everything(
    snapshot: Snapshot,
    now: datetime,
    budget: Budget,
    glyphs: Glyphs,
    keymap: Keymap,
) -> tuple[RenderableType, ...]:
    """Everything the interface could ever draw, for measuring against.

    The frame takes its width from the longest line anywhere: any row of any
    pane, on screen or scrolled away, and any set of reminders.  One width
    then serves the whole interface, so moving the cursor, scrolling, or
    changing tab never resizes it.
    """
    # Finished work stays on the task panes, so it is measured with the rest.
    every_task = _task_table(
        snapshot.tasks,
        range(len(snapshot.tasks)),
        None,
        snapshot,
        now,
        budget,
        glyphs,
        projects=True,
    )
    dated = agenda_rows(snapshot)
    agenda = _task_table(
        dated,
        range(len(dated)),
        None,
        snapshot,
        now,
        budget,
        glyphs,
        projects=True,
        countdown=True,
    )
    summaries = summary_rows(snapshot, now)
    tree = _summary_table(
        summaries,
        range(len(summaries)),
        SummaryScreen(None, Normal()),
        now,
        budget,
        glyphs,
    )
    return (every_task, agenda, tree, *_widest_hints(keymap, glyphs))


def _sized(
    build: Callable[[range], RenderableType],
    count: int,
    position: int,
    room: int,
) -> tuple[RenderableType, RenderableType]:
    """The rows to draw, and the same rows entire for measuring against."""
    visible = _window(count, position, room)
    drawn = build(visible)
    whole = drawn if len(visible) == count else build(range(count))
    return drawn, whole


def _place[Row](cursor: Row | None, rows: list[Row]) -> int:
    return rows.index(cursor) if cursor in rows else 0


@dataclass(frozen=True, slots=True)
class Frame:
    """Draws its parts at one shared width, over one ground.

    The width is that of the widest part, capped by the terminal, so the
    selected row spans the whole block instead of stopping wherever its own
    text happens to end.

    The ground goes underneath everything, which is what makes each pane
    recognisable before it has been read.  It is laid down first, so anything
    with a background of its own, such as the selected row, still shows.
    """

    parts: tuple[RenderableType, ...]
    anchors: tuple[RenderableType, ...] = ()
    trailer: RenderableType | None = None
    ground: str | None = None

    def __rich_console__(
        self,
        console: Console,
        options: ConsoleOptions,
    ) -> RenderResult:
        widest = max(
            (
                console.measure(part, options=options).maximum
                for part in (*self.parts, *self.anchors)
            ),
            default=1,
        )
        shared = options.update_width(max(1, min(widest, options.max_width)))
        drawn = self.parts if self.trailer is None else (*self.parts, self.trailer)
        under = None if self.ground is None else console.get_style(self.ground)
        for part in drawn:
            for line in console.render_lines(part, shared, pad=True):
                yield from line if under is None else Segment.apply_style(line, under)
                yield Segment.line()


def _window(count: int, position: int, size: int) -> range:
    """The slice of rows to draw, keeping ``position`` inside it."""
    if count <= size:
        return range(count)
    start = min(max(0, position - size // 2), count - size)
    return range(start, start + size)


def _title(
    state: State,
    trail: str | None,
    count: int,
    position: int,
    budget: Budget,
    glyphs: Glyphs,
) -> RenderableType | None:
    """The tab bar, and where in the list the cursor is.

    A tab does not repeat its own name; the trail appears only after drilling
    into something the tab name does not already say.
    """
    if not budget.title:
        return None
    return TabBar(
        chosen=state.tab,
        trail=trail,
        count=f"{position + 1}/{count}" if count else "empty",
        breadcrumb=glyphs.breadcrumb,
    )


@dataclass(frozen=True, slots=True)
class TabBar:
    """A bar running the full width, with the open tab filled in.

    The bar carries its own background across the whole line rather than
    stopping at the last label, so it reads as a bar and not as a run of
    coloured words.
    """

    chosen: Tab
    trail: str | None
    count: str
    breadcrumb: str

    def __rich_console__(
        self,
        console: Console,
        options: ConsoleOptions,
    ) -> RenderResult:
        bar = Text(style=theme.BAR, no_wrap=True, overflow="ellipsis")
        for tab in TAB_ORDER:
            picked = tab is self.chosen
            style = theme.TAB_CHOSEN if picked else theme.TAB
            bar.append(f" {TAB_LABELS[tab]} ", style=style)
        if self.trail is not None:
            bar.append(f" {self.breadcrumb} {self.trail}", style=theme.TRAIL)
        room = options.max_width - bar.cell_len - len(self.count) - 1
        if room >= 0:
            bar.append(" " * (room + 1))
            bar.append(self.count, style=theme.COUNT)
        else:
            bar.truncate(options.max_width, overflow="ellipsis")
        bar.pad_right(max(0, options.max_width - bar.cell_len))
        yield bar

    def __rich_measure__(
        self,
        console: Console,
        options: ConsoleOptions,
    ) -> Measurement:
        labels = sum(len(TAB_LABELS[tab]) + 2 for tab in TAB_ORDER)
        trail = 0 if self.trail is None else len(self.trail) + 3
        widest = labels + trail + len(self.count) + 1
        return Measurement(min(labels, options.max_width), widest)


def _list_title(screen: ListScreen) -> str:
    match screen.scope:
        case AllProjects():
            return "all tasks"
        case NoProject() | UnderProject():
            return scope_label(screen.scope)


def _grid(budget: Budget) -> Table:
    return Table(
        box=None,
        pad_edge=False,
        expand=True,
        show_header=bool(budget.header),
        header_style=theme.HEADER,
        padding=(0, 1),
    )


def _summary_table(
    rows: tuple[ProjectSummary, ...],
    visible: range,
    screen: SummaryScreen,
    now: datetime,
    budget: Budget,
    glyphs: Glyphs,
) -> RenderableType:
    if not rows:
        return Text("no projects yet", style=theme.MUTED)
    table = _grid(budget)
    table.add_column("project", no_wrap=True, overflow="ellipsis", ratio=1)
    table.add_column("open", justify="right", no_wrap=True)
    table.add_column("due", justify="right", no_wrap=True)
    table.add_column("tracked", justify="right", no_wrap=True)
    for index in visible:
        row = rows[index]
        chosen = row.scope == screen.cursor
        table.add_row(
            f"{glyphs.indent * row.depth}{row.label}",
            str(row.pending),
            _dated(format_due(row.due, now), row.due, now),
            _duration(row.tracked),
            style=theme.CURSOR if chosen else None,
        )
    return table


def _task_table(
    tasks: tuple[Task, ...],
    visible: range,
    cursor: TaskId | None,
    snapshot: Snapshot,
    now: datetime,
    budget: Budget,
    glyphs: Glyphs,
    *,
    projects: bool,
    countdown: bool = False,
) -> RenderableType:
    if not tasks:
        return Text("no tasks here", style=theme.MUTED)
    table = _grid(budget)
    table.add_column("", no_wrap=True, width=1)
    if projects:
        table.add_column("project", no_wrap=True, overflow="ellipsis")
    table.add_column("description", no_wrap=True, overflow="ellipsis", ratio=1)
    table.add_column("due", justify="right", no_wrap=True)
    if countdown:
        table.add_column("left", justify="right", no_wrap=True)
    table.add_column("tracked", justify="right", no_wrap=True)
    for index in visible:
        task = tasks[index]
        finished = isinstance(task.state, Done)
        due = snapshot.due_for(task)
        cells: list[str | Text] = [_marker(task, snapshot, glyphs)]
        if projects:
            cells.append("" if task.project is None else str(task.project))
        cells.append(task.description.text)
        # Finished work keeps its deadline on show as part of the record, but
        # stops being shouted about: there is nothing left to be late for.  A
        # countdown never meets one, since the agenda shows only what is left.
        cells.append(
            Text(format_due(due, now))
            if finished
            else _dated(format_due(due, now), due, now)
        )
        if countdown:
            cells.append(_dated(format_remaining(due, now), due, now))
        cells.append(_duration(snapshot.elapsed(task, now)))
        if finished:
            cells = [_faded(cell) for cell in cells]
        table.add_row(*cells, style=theme.CURSOR if task.id == cursor else None)
    return table


def _faded(cell: str | Text) -> Text:
    """A cell dimmed to say that its task is finished.

    The fade goes on the cells rather than on the row, so that the cursor,
    which sets a background and nothing else, settles over it.  Whatever
    colour a cell had is dropped: nothing on a finished row should compete
    with the work that is left.
    """
    return Text(cell if isinstance(cell, str) else cell.plain, style=theme.DONE)


def _marker(task: Task, snapshot: Snapshot, glyphs: Glyphs) -> Text:
    """The single character beside a task.

    A clock still running is worth more attention than a task being finished,
    and the two cannot happen at once, so one column carries both.
    """
    if snapshot.is_running(task.id):
        return Text(glyphs.active, style=theme.ACTIVE)
    if isinstance(task.state, Done):
        return Text(glyphs.done)
    return Text("")


def _dated(text: str, due: datetime | None, now: datetime) -> Text:
    """A deadline, shouted about only when it has already gone by.

    The cursor sets a background and nothing else, so this keeps its own
    colour on the selected row just as it does on any other.
    """
    if not text:
        return Text("")
    return Text(text, style=theme.OVERDUE if is_overdue(due, now) else theme.DUE)


def _hints(state: State, keymap: Keymap, glyphs: Glyphs) -> Hints:
    """The reminders for the pane in front, and for the row under the cursor."""
    match state.screen:
        case AgendaScreen():
            parts = task_hints(keymap, nested=False)
        case SummaryScreen() as screen:
            on_project = isinstance(screen.cursor, UnderProject)
            parts = project_hints(keymap, on_project=on_project)
        case ListScreen():
            parts = task_hints(keymap, nested=True)
    return Hints(tuple(parts), gap=glyphs.gap)


def _widest_hints(keymap: Keymap, glyphs: Glyphs) -> tuple[Hints, ...]:
    """Every set of reminders the interface could show.

    The frame is measured against all of them so that it keeps one width, and
    moving the cursor onto a row with fewer reminders does not resize it.
    """
    return (
        Hints(tuple(task_hints(keymap, nested=True)), gap=glyphs.gap),
        Hints(tuple(project_hints(keymap, on_project=True)), gap=glyphs.gap),
    )


def _status(state: State, glyphs: Glyphs) -> RenderableType:
    """The bottom line: what is being typed, or what just happened.

    Everything transient lives here and nowhere else, so the rest of the frame
    holds still.
    """
    if state.pending:
        return Text(sequence_label(state.pending), style=theme.TITLE, no_wrap=True)
    typed = _prompted(state, glyphs)
    if typed is not None:
        return typed
    match state.status:
        case Problem(text):
            return Text(text, style=theme.PROBLEM, no_wrap=True, overflow="ellipsis")
        case Note(text):
            return Text(text, style=theme.NOTE, no_wrap=True, overflow="ellipsis")
        case None:
            return Text("")


def _prompted(state: State, glyphs: Glyphs) -> Text | None:
    """The field being typed into, if one is open."""
    match state.screen:
        case SummaryScreen(_, mode):
            return _summary_prompt(mode, glyphs)
        case AgendaScreen(_, mode) | ListScreen(_, _, mode):
            return _task_prompt(mode, glyphs)


def _summary_prompt(mode: SummaryMode, glyphs: Glyphs) -> Text | None:
    match mode:
        case Renaming(target, buffer):
            return _prompt(f"rename {target}", buffer, glyphs)
        case Naming(buffer):
            return _prompt("new project", buffer, glyphs)
        case SchedulingProject(target, buffer):
            return _prompt(f"{target} due", buffer, glyphs)
        case Jumping():
            return Text("jump to project starting with...", style=theme.MUTED)
        case Normal():
            return None


def _task_prompt(mode: ListMode, glyphs: Glyphs) -> Text | None:
    match mode:
        case Adding(buffer):
            return _prompt("add", buffer, glyphs)
        case Editing(_, buffer):
            return _prompt("edit", buffer, glyphs)
        case Reprojecting(_, buffer):
            return _prompt("project (blank for none)", buffer, glyphs)
        case Scheduling(_, buffer):
            return _prompt(f"due ({EXAMPLES})", buffer, glyphs)
        case Normal():
            return None


def _fill(parts: list[Hint], target: int, gap: str) -> list[str] | None:
    """Lay reminders out in order, breaking at ``target`` columns."""
    rows: list[str] = []
    current = ""
    for part in parts:
        if len(part.text) > target:
            return None
        joined = f"{current}{gap}{part.text}" if current else part.text
        if len(joined) <= target:
            current = joined
            continue
        rows.append(current)
        current = part.text
    if current:
        rows.append(current)
    return rows


def _pack(parts: list[Hint], width: int, limit: int, gap: str) -> list[str] | None:
    """Lay reminders out over at most ``limit`` lines, or fail if they spill.

    The narrowest layout that still fits is chosen, which spreads the
    reminders evenly instead of leaving one stranded on a line of its own.
    """
    if not parts:
        return []
    longest = max(len(part.text) for part in parts)
    if longest > width:
        return None
    for target in range(longest, width + 1):
        rows = _fill(parts, target, gap)
        if rows is not None and len(rows) <= limit:
            return rows
    return None


@dataclass(frozen=True, slots=True)
class Hints:
    """Key reminders, wrapped over a second line before any are given up.

    A reminder cut in half would be worse than one left out, so whole
    reminders go, and only ones that can be spared.
    """

    parts: tuple[Hint, ...]
    gap: str = DEFAULT_GLYPHS.gap
    limit: int = HINT_LINES

    def lines(self, width: int) -> list[str]:
        kept = list(self.parts)
        while True:
            packed = _pack(kept, width, self.limit, self.gap)
            if packed is not None:
                return packed
            expendable = [part for part in kept if not part.essential]
            if not expendable:
                joined = self.gap.join(part.text for part in kept)
                return [joined[:width]] if width > 0 else []
            kept.remove(expendable[-1])

    def height(self, width: int) -> int:
        """How many lines the reminders would like, up to their limit."""
        return max(1, len(self.lines(width)))

    def __rich_console__(
        self,
        console: Console,
        options: ConsoleOptions,
    ) -> RenderResult:
        for row in self.lines(options.max_width):
            yield Text(row, style=theme.MUTED, no_wrap=True)

    def __rich_measure__(
        self,
        console: Console,
        options: ConsoleOptions,
    ) -> Measurement:
        longest = max((len(part.text) for part in self.parts), default=0)
        whole = len(self.gap.join(part.text for part in self.parts))
        return Measurement(min(longest, options.max_width), whole)


def _prompt(label: str, buffer: str, glyphs: Glyphs) -> Text:
    text = Text(no_wrap=True, overflow="ellipsis")
    text.append(f"{label}: ", style=theme.TITLE)
    text.append(buffer)
    text.append(glyphs.caret, style=theme.CARET)
    return text


def _duration(span: timedelta) -> str:
    """A compact rendering such as ``2h30`` or ``45m``."""
    seconds = int(span.total_seconds())
    if seconds <= 0:
        return ""
    hours, remainder = divmod(seconds, 3600)
    minutes = remainder // 60
    if hours:
        return f"{hours}h{minutes:02d}"
    if minutes:
        return f"{minutes}m"
    return f"{seconds}s"
