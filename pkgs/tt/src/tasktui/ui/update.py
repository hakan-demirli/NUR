"""How a key press changes what is on screen.

This module is pure.  It reads a snapshot and returns the next state together
with whatever the store should be asked to do, so every binding can be tested
without a terminal or a database.

A key press in a pane's resting state is first folded into whatever keys are
already waiting, which is how a run such as ``g n`` comes to mean one thing
while ``g`` alone means nothing.
"""

from __future__ import annotations

from dataclasses import replace
from datetime import datetime
from enum import Enum
from typing import Final

from tasktui.task.due import InvalidDeadline, as_input, parse_due
from tasktui.task.model import (
    AllProjects,
    Description,
    Done,
    InvalidDescription,
    InvalidProjectPath,
    NoProject,
    Pending,
    ProjectFilter,
    ProjectPath,
    ProjectSummary,
    Snapshot,
    Task,
    TaskId,
    UnderProject,
    row_label,
    scope_label,
    summarize,
    summary_scopes,
)
from tasktui.term.keys import Char, Control, Key, KeyPress
from tasktui.ui.action import (
    Bindings,
    Bound,
    Keymap,
    Partial,
    ProjectAction,
    TaskAction,
    Unbound,
)
from tasktui.ui.state import (
    Adding,
    AddTask,
    AgendaScreen,
    Command,
    Complete,
    Editing,
    ForgetProject,
    Jumping,
    ListMode,
    ListScreen,
    MakeProject,
    Moved,
    Naming,
    Normal,
    Note,
    Outcome,
    Problem,
    ProjectsScreen,
    Quit,
    RenameProject,
    Renaming,
    Reopen,
    Reproject,
    Reprojecting,
    Reschedule,
    RescheduleProject,
    Retitle,
    Scheduling,
    SchedulingProject,
    StartClock,
    State,
    Status,
    StopClock,
    SummaryMode,
    SummaryScreen,
    SwapPlaces,
    SwapProjectPlaces,
    Tab,
)

DEFAULT_PAGE: Final = 10
QUIT: tuple[Command, ...] = (Quit(),)
NOTHING: tuple[Command, ...] = ()
Transition = tuple[State, tuple[Command, ...]]
Typing = Adding | Editing | Reprojecting | Scheduling


def initial_state(scope: ProjectFilter | None) -> State:
    """The screen to open on.

    Given a scope the interface opens that project's task list on the projects
    tab, otherwise it opens on the agenda.
    """
    agenda = AgendaScreen(cursor=None, mode=Normal())
    if scope is None:
        return State(agenda=agenda, projects=SummaryScreen(None, Normal()))
    return State(
        agenda=agenda,
        projects=ListScreen(scope=scope, cursor=None, mode=Normal()),
        tab=Tab.PROJECTS,
    )


def summary_rows(snapshot: Snapshot, now: datetime) -> tuple[ProjectSummary, ...]:
    """The rows of the project pane, with what each one holds."""
    return summarize(snapshot, now)


def list_rows(snapshot: Snapshot, screen: ListScreen) -> tuple[Task, ...]:
    """The rows of the task pane, in the order the list has been put in.

    Finished work stays exactly where it was rather than being gathered up
    somewhere, so an order arranged by hand survives ticking something off.
    """
    return tuple(task for task in snapshot.tasks if task.matches(screen.scope))


def agenda_rows(snapshot: Snapshot) -> tuple[Task, ...]:
    """Everything still to do that has a deadline, soonest first."""
    dated = [
        (snapshot.due_for(task), task)
        for task in snapshot.tasks
        if isinstance(task.state, Pending)
    ]
    return tuple(
        task
        for due, task in sorted(
            ((due, task) for due, task in dated if due is not None),
            key=lambda pair: (pair[0], pair[1].id),
        )
    )


def rows_of(state: State, snapshot: Snapshot) -> tuple[Task, ...]:
    """The tasks shown by whichever task pane is in front."""
    match state.screen:
        case AgendaScreen():
            return agenda_rows(snapshot)
        case ListScreen() as screen:
            return list_rows(snapshot, screen)
        case SummaryScreen():
            return ()


def reanchor(state: State, snapshot: Snapshot) -> State:
    """Point every cursor at a row that still exists."""
    agenda = _anchored_task(state.agenda, agenda_rows(snapshot))
    projects = state.projects
    match projects:
        case SummaryScreen() as screen:
            scopes = list(summary_scopes(snapshot))
            if screen.cursor not in scopes:
                projects = SummaryScreen(scopes[0] if scopes else None, screen.mode)
        case ListScreen() as screen:
            projects = _anchored_list(screen, list_rows(snapshot, screen))
    return replace(state, agenda=agenda, projects=projects)


def _anchored_task(screen: AgendaScreen, rows: tuple[Task, ...]) -> AgendaScreen:
    identifiers = [task.id for task in rows]
    if screen.cursor in identifiers:
        return screen
    return AgendaScreen(identifiers[0] if identifiers else None, screen.mode)


def _anchored_list(screen: ListScreen, rows: tuple[Task, ...]) -> ListScreen:
    identifiers = [task.id for task in rows]
    if screen.cursor in identifiers:
        return screen
    selected = identifiers[0] if identifiers else None
    return ListScreen(screen.scope, selected, screen.mode)


def update(
    press: KeyPress,
    state: State,
    snapshot: Snapshot,
    keymap: Keymap,
    now: datetime,
    page: int = DEFAULT_PAGE,
) -> Transition:
    """Fold a key press into the state, and say what the store should do.

    ``page`` is how far a paging key travels, which is half of whatever the
    pane can show.
    """
    if press is Key.INTERRUPT:
        return state, QUIT
    match state.screen:
        case AgendaScreen() as screen:
            return _agenda(press, state, screen, snapshot, keymap, now, page)
        case SummaryScreen() as screen:
            return _summary(press, state, screen, snapshot, keymap, now, page)
        case ListScreen() as screen:
            return _list(press, state, screen, snapshot, keymap, now, page)


def focus(state: State, task_id: TaskId) -> State:
    """Select a task, so that a freshly added one is ready to act on."""
    match state.screen:
        case AgendaScreen() as screen:
            return replace(state, agenda=AgendaScreen(task_id, screen.mode))
        case ListScreen() as screen:
            return replace(
                state,
                projects=ListScreen(screen.scope, task_id, screen.mode),
            )
        case SummaryScreen():
            return state


def describe(command: Command, outcome: Outcome) -> Status | None:
    """The message to show once a command has been carried out."""
    match command:
        case AddTask():
            return Note("added")
        case Retitle():
            return Note("description changed")
        case Reproject(_, project):
            return Note(f"moved to {project or 'no project'}")
        case Reschedule(_, due):
            return Note("deadline set" if due else "deadline cleared")
        case RescheduleProject(project, due):
            settled = "set on" if due else "cleared from"
            return Note(f"deadline {settled} {project}")
        case Complete():
            return Note("done")
        case Reopen():
            return Note("reopened")
        case SwapPlaces() | SwapProjectPlaces():
            return None
        case StartClock():
            return Note("clock started")
        case StopClock():
            return Note("clock stopped")
        case MakeProject(project):
            return Note(f"made {project}")
        case ForgetProject(project):
            return Note(f"forgot {project}")
        case RenameProject(old, new):
            count = outcome.count if isinstance(outcome, Moved) else 0
            plural = "task" if count == 1 else "tasks"
            return Note(f"moved {count} {plural} from {old} to {new}")
        case Quit():
            return None


def _chord[A: Enum](
    bindings: Bindings[A],
    state: State,
    press: KeyPress,
) -> tuple[A | None, State]:
    """Resolve a key against whatever keys are already waiting."""
    match bindings.resolve(state.pending, press):
        case Bound(action):
            return action, replace(state, pending=(), status=None)
        case Partial(keys):
            return None, replace(state, pending=keys, status=None)
        case Unbound():
            return None, replace(state, pending=(), status=None)


def _agenda(
    press: KeyPress,
    state: State,
    screen: AgendaScreen,
    snapshot: Snapshot,
    keymap: Keymap,
    now: datetime,
    page: int,
) -> Transition:
    match screen.mode:
        case Normal():
            action, settled = _chord(keymap.tasks, state, press)
            rows = agenda_rows(snapshot)
            return _task_action(action, settled, screen, rows, snapshot, now, page)
        case Adding() | Editing() | Reprojecting() | Scheduling() as mode:
            return _typing(press, state, screen, mode, now)


def _list(
    press: KeyPress,
    state: State,
    screen: ListScreen,
    snapshot: Snapshot,
    keymap: Keymap,
    now: datetime,
    page: int,
) -> Transition:
    match screen.mode:
        case Normal():
            action, settled = _chord(keymap.tasks, state, press)
            rows = list_rows(snapshot, screen)
            return _task_action(action, settled, screen, rows, snapshot, now, page)
        case Adding() | Editing() | Reprojecting() | Scheduling() as mode:
            return _typing(press, state, screen, mode, now)


def _task_action(
    action: TaskAction | None,
    state: State,
    screen: AgendaScreen | ListScreen,
    rows: tuple[Task, ...],
    snapshot: Snapshot,
    now: datetime,
    page: int,
) -> Transition:
    identifiers = [task.id for task in rows]
    selected = next((task for task in rows if task.id == screen.cursor), None)
    match action:
        case TaskAction.QUIT:
            return state, QUIT
        case TaskAction.NEXT_TAB:
            return state.stepped(1), NOTHING
        case TaskAction.PREVIOUS_TAB:
            return state.stepped(-1), NOTHING
        case TaskAction.MOVE_DOWN:
            return _at(state, screen, _step(identifiers, screen.cursor, 1)), NOTHING
        case TaskAction.MOVE_UP:
            return _at(state, screen, _step(identifiers, screen.cursor, -1)), NOTHING
        case TaskAction.PAGE_DOWN:
            return _at(state, screen, _leap(identifiers, screen.cursor, page)), NOTHING
        case TaskAction.PAGE_UP:
            return _at(state, screen, _leap(identifiers, screen.cursor, -page)), NOTHING
        case TaskAction.MOVE_TOP:
            return _at(state, screen, _edge(identifiers, first=True)), NOTHING
        case TaskAction.MOVE_END:
            return _at(state, screen, _edge(identifiers, first=False)), NOTHING
        case TaskAction.BACK if isinstance(screen, ListScreen):
            return state.showing(SummaryScreen(screen.scope, Normal())), NOTHING
        case TaskAction.ADD if isinstance(screen, ListScreen):
            return _in(state, screen, Adding(buffer="")), NOTHING
        case TaskAction.ADD:
            # The agenda shows what already has a deadline. A task made here
            # would have neither deadline nor project, so it would vanish the
            # moment it was made; there is nowhere for it to go.
            return _complain(
                state, "nothing is added here; add it in a project"
            ), NOTHING
        case (
            TaskAction.EDIT
            | TaskAction.SET_PROJECT
            | TaskAction.SET_DUE
            | TaskAction.TOGGLE_CLOCK
        ) if selected is not None and isinstance(selected.state, Done):
            # The store will not change a finished task, so saying so now
            # beats taking a whole line of typing first and refusing it after.
            return _complain(state, "that is finished; reopen it to change it"), NOTHING
        case TaskAction.EDIT if selected is not None:
            mode = Editing(selected.id, selected.description.text)
            return _in(state, screen, mode), NOTHING
        case TaskAction.SET_PROJECT if selected is not None:
            current = "" if selected.project is None else str(selected.project)
            return _in(state, screen, Reprojecting(selected.id, current)), NOTHING
        case TaskAction.SET_DUE if selected is not None:
            written = as_input(selected.due, now)
            return _in(state, screen, Scheduling(selected.id, written)), NOTHING
        case TaskAction.SHIFT_DOWN if isinstance(screen, ListScreen):
            return _shifted(state, rows, screen.cursor, 1)
        case TaskAction.SHIFT_UP if isinstance(screen, ListScreen):
            return _shifted(state, rows, screen.cursor, -1)
        case TaskAction.SHIFT_DOWN | TaskAction.SHIFT_UP:
            return _complain(state, "the agenda is in deadline order"), NOTHING
        case TaskAction.COMPLETE if selected is not None:
            return _turn_over(state, screen, rows, selected)
        case TaskAction.TOGGLE_CLOCK if selected is not None:
            # Several clocks may run at once, so this only turns over the one
            # belonging to the task under the cursor.
            command: Command = (
                StopClock(selected.id)
                if snapshot.is_running(selected.id)
                else StartClock(selected.id)
            )
            return state, (command,)
        case _:
            return state, NOTHING


def _summary(
    press: KeyPress,
    state: State,
    screen: SummaryScreen,
    snapshot: Snapshot,
    keymap: Keymap,
    now: datetime,
    page: int,
) -> Transition:
    match screen.mode:
        case Normal():
            action, settled = _chord(keymap.projects, state, press)
            return _project_action(action, settled, screen, snapshot, now, page)
        case Jumping():
            return _jump(press, state, screen, snapshot)
        case Renaming() as mode:
            return _renaming(press, state, screen, mode)
        case Naming() as mode:
            return _naming(press, state, screen, mode)
        case SchedulingProject() as mode:
            return _project_scheduling(press, state, screen, mode, now)


def _project_action(
    action: ProjectAction | None,
    state: State,
    screen: SummaryScreen,
    snapshot: Snapshot,
    now: datetime,
    page: int,
) -> Transition:
    scopes = list(summary_scopes(snapshot))
    match action:
        case ProjectAction.QUIT:
            return state, QUIT
        case ProjectAction.NEXT_TAB:
            return state.stepped(1), NOTHING
        case ProjectAction.PREVIOUS_TAB:
            return state.stepped(-1), NOTHING
        case ProjectAction.MOVE_DOWN:
            return _scoped(state, _step(scopes, screen.cursor, 1)), NOTHING
        case ProjectAction.MOVE_UP:
            return _scoped(state, _step(scopes, screen.cursor, -1)), NOTHING
        case ProjectAction.PAGE_DOWN:
            return _scoped(state, _leap(scopes, screen.cursor, page)), NOTHING
        case ProjectAction.PAGE_UP:
            return _scoped(state, _leap(scopes, screen.cursor, -page)), NOTHING
        case ProjectAction.MOVE_TOP:
            return _scoped(state, _edge(scopes, first=True)), NOTHING
        case ProjectAction.MOVE_END:
            return _scoped(state, _edge(scopes, first=False)), NOTHING
        case ProjectAction.OPEN:
            if screen.cursor is None:
                return state, NOTHING
            opened = ListScreen(scope=screen.cursor, cursor=None, mode=Normal())
            return state.showing(opened), NOTHING
        case ProjectAction.SHIFT_DOWN:
            return _shifted_project(state, screen, scopes, 1)
        case ProjectAction.SHIFT_UP:
            return _shifted_project(state, screen, scopes, -1)
        case ProjectAction.NEW:
            return state.showing(SummaryScreen(screen.cursor, Naming(""))), NOTHING
        case ProjectAction.FORGET:
            return _forget(state, screen)
        case ProjectAction.FIND:
            return state.showing(SummaryScreen(screen.cursor, Jumping())), NOTHING
        case ProjectAction.RENAME:
            return _begin_rename(state, screen), NOTHING
        case ProjectAction.SET_DUE:
            return _begin_project_due(state, screen, snapshot, now), NOTHING
        case None:
            return state, NOTHING


def _row(cursor: ProjectFilter | None) -> str:
    """How a tree row is referred to when talking about it."""
    return "nothing" if cursor is None else f"{scope_label(cursor)!r}"


def _shifted_project(
    state: State,
    screen: SummaryScreen,
    scopes: list[ProjectFilter],
    delta: int,
) -> Transition:
    """Move the project under the cursor one place along.

    The exchange is with the next project alongside it rather than with the
    next row: the row below a project is usually the first thing inside it,
    and a project cannot be put inside itself.  Everything beneath it comes
    along, so a whole branch moves at once.  At either end of a set of
    neighbours there is nothing to exchange with and nothing happens.
    """
    if not isinstance(screen.cursor, UnderProject):
        return _complain(state, f"{_row(screen.cursor)} is not a project"), NOTHING
    path = screen.cursor.path
    alongside = [
        scope.path
        for scope in scopes
        if isinstance(scope, UnderProject) and scope.path.parent == path.parent
    ]
    wanted = alongside.index(path) + delta
    if not 0 <= wanted < len(alongside):
        return state, NOTHING
    return state, (SwapProjectPlaces(first=path, second=alongside[wanted]),)


def _forget(state: State, screen: SummaryScreen) -> Transition:
    """Ask for a project to be dropped; the store refuses if it holds work."""
    match screen.cursor:
        case UnderProject(path):
            calm = state.showing(SummaryScreen(screen.cursor, Normal()))
            return calm, (ForgetProject(path),)
        case _:
            return _complain(state, f"{_row(screen.cursor)} is not a project"), NOTHING


def _naming(
    press: KeyPress,
    state: State,
    screen: SummaryScreen,
    mode: Naming,
) -> Transition:
    calm = SummaryScreen(screen.cursor, Normal())
    match press:
        case Key.ESCAPE:
            return state.showing(calm), NOTHING
        case Key.ENTER:
            try:
                made = ProjectPath.parse(mode.buffer)
            except InvalidProjectPath as error:
                return _complain(state.showing(calm), str(error)), NOTHING
            moved = state.showing(SummaryScreen(UnderProject(made), Normal()))
            return moved, (MakeProject(made),)
        case _:
            typed = _typed(mode.buffer, press)
            if typed is None:
                return state, NOTHING
            replaced = SummaryScreen(screen.cursor, Naming(typed))
            return state.showing(replaced), NOTHING


def _scoped(state: State, cursor: ProjectFilter | None) -> State:
    return state.showing(SummaryScreen(cursor, Normal()))


def _begin_rename(state: State, screen: SummaryScreen) -> State:
    match screen.cursor:
        case UnderProject(path):
            mode = Renaming(target=path, buffer=path.name)
            return state.showing(SummaryScreen(screen.cursor, mode))
        case _:
            return _complain(state, f"{_row(screen.cursor)} is not a project to rename")


def _begin_project_due(
    state: State,
    screen: SummaryScreen,
    snapshot: Snapshot,
    now: datetime,
) -> State:
    match screen.cursor:
        case UnderProject(path):
            written = as_input(snapshot.project_due.get(path), now)
            mode = SchedulingProject(target=path, buffer=written)
            return state.showing(SummaryScreen(screen.cursor, mode))
        case _:
            # A deadline hangs on a project or on a task. This row is neither;
            # it stands for a selection of them.
            return _complain(
                state,
                f"{_row(screen.cursor)} is a selection, not a project; "
                "set the deadline on a task, or on a project row",
            )


def _jump(
    press: KeyPress,
    state: State,
    screen: SummaryScreen,
    snapshot: Snapshot,
) -> Transition:
    scopes = list(summary_scopes(snapshot))
    calm = SummaryScreen(screen.cursor, Normal())
    if not isinstance(press, Char) or not scopes:
        return state.showing(calm), NOTHING
    start = scopes.index(screen.cursor) + 1 if screen.cursor in scopes else 0
    wanted = press.value.lower()
    for offset in range(len(scopes)):
        index = (start + offset) % len(scopes)
        if row_label(scopes[index]).lower().startswith(wanted):
            return state.showing(SummaryScreen(scopes[index], Normal())), NOTHING
    settled = state.showing(calm)
    return _complain(settled, f"no project starting with {press.value!r}"), NOTHING


def _renaming(
    press: KeyPress,
    state: State,
    screen: SummaryScreen,
    mode: Renaming,
) -> Transition:
    calm = SummaryScreen(screen.cursor, Normal())
    match press:
        case Key.ESCAPE:
            return state.showing(calm), NOTHING
        case Key.ENTER:
            return _commit_rename(state, calm, mode)
        case _:
            typed = _typed(mode.buffer, press)
            if typed is None:
                return state, NOTHING
            replaced = Renaming(mode.target, typed)
            return state.showing(SummaryScreen(screen.cursor, replaced)), NOTHING


def _commit_rename(state: State, calm: SummaryScreen, mode: Renaming) -> Transition:
    parent = mode.target.parent
    text = mode.buffer if parent is None else f"{parent}.{mode.buffer}"
    try:
        renamed = ProjectPath.parse(text)
    except InvalidProjectPath as error:
        return _complain(state.showing(calm), str(error)), NOTHING
    if renamed == mode.target:
        return state.showing(calm), NOTHING
    moved = state.showing(SummaryScreen(UnderProject(renamed), Normal()))
    return moved, (RenameProject(old=mode.target, new=renamed),)


def _project_scheduling(
    press: KeyPress,
    state: State,
    screen: SummaryScreen,
    mode: SchedulingProject,
    now: datetime,
) -> Transition:
    calm = SummaryScreen(screen.cursor, Normal())
    match press:
        case Key.ESCAPE:
            return state.showing(calm), NOTHING
        case Key.ENTER:
            try:
                due = parse_due(mode.buffer, now)
            except InvalidDeadline as error:
                return _complain(state.showing(calm), str(error)), NOTHING
            return state.showing(calm), (RescheduleProject(mode.target, due),)
        case _:
            typed = _typed(mode.buffer, press)
            if typed is None:
                return state, NOTHING
            replaced = SchedulingProject(mode.target, typed)
            return state.showing(SummaryScreen(screen.cursor, replaced)), NOTHING


def _typing(
    press: KeyPress,
    state: State,
    screen: AgendaScreen | ListScreen,
    mode: Typing,
    now: datetime,
) -> Transition:
    match press:
        case Key.ESCAPE:
            return _at(state, screen, screen.cursor), NOTHING
        case Key.ENTER:
            return _commit_typing(state, screen, mode, now)
        case _:
            typed = _typed(mode.buffer, press)
            if typed is None:
                return state, NOTHING
            return _in(state, screen, _rebuffered(mode, typed)), NOTHING


def _commit_typing(
    state: State,
    screen: AgendaScreen | ListScreen,
    mode: Typing,
    now: datetime,
) -> Transition:
    calm = _at(state, screen, screen.cursor)
    match mode:
        case Reprojecting(target, buffer):
            return _commit_project(state, calm, target, buffer)
        case Scheduling(target, buffer):
            try:
                due = parse_due(buffer, now)
            except InvalidDeadline as error:
                return _complain(calm, str(error)), NOTHING
            return calm, (Reschedule(target, due),)
        case Adding() | Editing():
            try:
                description = Description.parse(mode.buffer)
            except InvalidDescription as error:
                return _complain(calm, str(error)), NOTHING
            if isinstance(mode, Adding):
                return calm, (AddTask(description, _project(screen)),)
            return calm, (Retitle(mode.target, description),)


def _commit_project(
    state: State,
    calm: State,
    target: TaskId,
    buffer: str,
) -> Transition:
    """An empty project takes the task out of the tree altogether."""
    if not buffer.strip():
        return calm, (Reproject(target, None),)
    try:
        project = ProjectPath.parse(buffer)
    except InvalidProjectPath as error:
        return _complain(calm, str(error)), NOTHING
    return calm, (Reproject(target, project),)


def _rebuffered(mode: Typing, buffer: str) -> Typing:
    match mode:
        case Adding():
            return Adding(buffer)
        case Editing(target, _):
            return Editing(target, buffer)
        case Reprojecting(target, _):
            return Reprojecting(target, buffer)
        case Scheduling(target, _):
            return Scheduling(target, buffer)


def _project(screen: AgendaScreen | ListScreen) -> ProjectPath | None:
    match screen:
        case AgendaScreen():
            return None
        case ListScreen(scope):
            match scope:
                case UnderProject(path):
                    return path
                case AllProjects() | NoProject():
                    return None


def _typed(buffer: str, press: KeyPress) -> str | None:
    """Apply a key press to a text buffer, or ``None`` if it was not text.

    Control with u and w erase, as they do at any other prompt.  They are not
    rebindable here: a field takes text, and the erasing keys come with it.
    """
    match press:
        case Key.BACKSPACE:
            return buffer[:-1]
        case Control("w"):
            return buffer[: len(buffer.rstrip().rpartition(" ")[0])].rstrip()
        case Control("u"):
            return ""
        case Char(value):
            return buffer + value
        case _:
            return None


def _step[Row](rows: list[Row], cursor: Row | None, delta: int) -> Row | None:
    """The row ``delta`` places from the cursor, wrapping at both ends."""
    if not rows:
        return None
    if cursor is None or cursor not in rows:
        return rows[0]
    return rows[(rows.index(cursor) + delta) % len(rows)]


def _shifted(
    state: State,
    rows: tuple[Task, ...],
    cursor: TaskId | None,
    delta: int,
) -> Transition:
    """Move the task under the cursor one place along the list.

    The exchange is with the row next to it on this pane, which may not be the
    task next to it in the list as a whole: a pane showing one project passes
    over everything it is not showing.  At either end there is nothing to
    exchange with and nothing happens.  The cursor names a task rather than a
    row, so it travels with it and the key can just be pressed again.
    """
    identifiers = [task.id for task in rows]
    if cursor is None or cursor not in identifiers:
        return state, NOTHING
    wanted = identifiers.index(cursor) + delta
    if not 0 <= wanted < len(identifiers):
        return state, NOTHING
    return state, (SwapPlaces(first=cursor, second=identifiers[wanted]),)


def _turn_over(
    state: State,
    screen: AgendaScreen | ListScreen,
    rows: tuple[Task, ...],
    selected: Task,
) -> Transition:
    """Finish the task under the cursor, or put a finished one back.

    A finished task keeps its place on the pane, so the cursor is moved on to
    the row below deliberately.  Several can be ticked off with one key that
    way, and a second press cannot undo the first by accident.
    """
    if isinstance(selected.state, Done):
        return state, (Reopen(task_id=selected.id),)
    identifiers = [task.id for task in rows]
    following = identifiers.index(selected.id) + 1
    moved = identifiers[following] if following < len(identifiers) else selected.id
    return _at(state, screen, moved), (Complete(task_id=selected.id),)


def _leap[Row](rows: list[Row], cursor: Row | None, delta: int) -> Row | None:
    """The row ``delta`` places away, stopping at the ends rather than wrapping.

    Wrapping is right for a single step, where it is asked for again and again
    and the ends are obvious.  Half a screen at a time it would throw the
    cursor to the far end of a long list without warning.
    """
    if not rows:
        return None
    if cursor is None or cursor not in rows:
        return rows[0]
    wanted = rows.index(cursor) + delta
    return rows[max(0, min(wanted, len(rows) - 1))]


def _edge[Row](rows: list[Row], *, first: bool) -> Row | None:
    if not rows:
        return None
    return rows[0] if first else rows[-1]


def _at(
    state: State,
    screen: AgendaScreen | ListScreen,
    cursor: TaskId | None,
) -> State:
    match screen:
        case AgendaScreen():
            return state.showing(AgendaScreen(cursor, Normal()))
        case ListScreen(scope):
            return state.showing(ListScreen(scope, cursor, Normal()))


def _in(
    state: State,
    screen: AgendaScreen | ListScreen,
    mode: ListMode,
) -> State:
    match screen:
        case AgendaScreen():
            return state.showing(AgendaScreen(screen.cursor, mode))
        case ListScreen(scope):
            return state.showing(ListScreen(scope, screen.cursor, mode))


def _complain(state: State, message: str) -> State:
    return replace(state, status=Problem(message))


__all__ = [
    "ListMode",
    "ProjectsScreen",
    "SummaryMode",
    "Transition",
    "agenda_rows",
    "describe",
    "focus",
    "initial_state",
    "list_rows",
    "reanchor",
    "rows_of",
    "summary_rows",
    "update",
]
