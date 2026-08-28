"""What is on screen, and what the interface has asked the store to do.

Modes carry the data they need, so a text field cannot exist without its
buffer and an edit cannot exist without knowing what it is editing.  The list
cursor holds a task identifier rather than a row number, so a task appearing
or disappearing does not move the selection to a different task.
"""

from __future__ import annotations

from dataclasses import dataclass, replace
from datetime import datetime
from enum import Enum
from typing import Final

from tasktui.task.model import (
    Description,
    ProjectFilter,
    ProjectPath,
    TaskId,
)
from tasktui.term.keys import KeyPress


@dataclass(frozen=True, slots=True)
class Normal:
    """Key presses are commands rather than text."""


@dataclass(frozen=True, slots=True)
class Renaming:
    """A project is being given a new name."""

    target: ProjectPath
    buffer: str


@dataclass(frozen=True, slots=True)
class Jumping:
    """The next key press selects the project it starts with."""


@dataclass(frozen=True, slots=True)
class Naming:
    """A new project is being named."""

    buffer: str


@dataclass(frozen=True, slots=True)
class SchedulingProject:
    """A deadline is being given to a project."""

    target: ProjectPath
    buffer: str


SummaryMode = Normal | Renaming | Jumping | Naming | SchedulingProject


@dataclass(frozen=True, slots=True)
class Adding:
    """A new task is being typed."""

    buffer: str


@dataclass(frozen=True, slots=True)
class Editing:
    """An existing task is being retyped."""

    target: TaskId
    buffer: str


@dataclass(frozen=True, slots=True)
class Reprojecting:
    """A task is being moved to a project, which may not exist yet."""

    target: TaskId
    buffer: str


@dataclass(frozen=True, slots=True)
class Scheduling:
    """A deadline is being given to a task."""

    target: TaskId
    buffer: str


ListMode = Normal | Adding | Editing | Reprojecting | Scheduling


@dataclass(frozen=True, slots=True)
class AgendaScreen:
    """Everything with a deadline, soonest first."""

    cursor: TaskId | None
    mode: ListMode


@dataclass(frozen=True, slots=True)
class SummaryScreen:
    """The project tree."""

    cursor: ProjectFilter | None
    mode: SummaryMode


@dataclass(frozen=True, slots=True)
class ListScreen:
    """The tasks of one project scope."""

    scope: ProjectFilter
    cursor: TaskId | None
    mode: ListMode


Screen = AgendaScreen | SummaryScreen | ListScreen
ProjectsScreen = SummaryScreen | ListScreen


class Tab(Enum):
    """One of the fixed views the interface is split into."""

    AGENDA = "agenda"
    PROJECTS = "projects"


TAB_ORDER: Final[tuple[Tab, ...]] = (Tab.AGENDA, Tab.PROJECTS)
TAB_LABELS: Final[dict[Tab, str]] = {
    Tab.AGENDA: "agenda",
    Tab.PROJECTS: "projects",
}


@dataclass(frozen=True, slots=True)
class Note:
    """Something worth telling the user about."""

    text: str


@dataclass(frozen=True, slots=True)
class Problem:
    """Something the user asked for that could not be done."""

    text: str


Status = Note | Problem


@dataclass(frozen=True, slots=True)
class State:
    """Everything that survives from one key press to the next.

    Each tab keeps its own screen, so moving away and back returns to what was
    left behind rather than starting over.
    """

    agenda: AgendaScreen
    projects: ProjectsScreen
    tab: Tab = Tab.AGENDA
    pending: tuple[KeyPress, ...] = ()
    status: Status | None = None

    @property
    def screen(self) -> Screen:
        """Whichever screen the current tab is showing."""
        return self.agenda if self.tab is Tab.AGENDA else self.projects

    def showing(self, screen: Screen) -> State:
        """The same state with the current tab's screen replaced."""
        match screen:
            case AgendaScreen():
                return replace(self, agenda=screen, pending=(), status=None)
            case SummaryScreen() | ListScreen():
                return replace(self, projects=screen, pending=(), status=None)

    def stepped(self, distance: int) -> State:
        """Move to another tab, wrapping at either end."""
        place = TAB_ORDER.index(self.tab)
        moved = TAB_ORDER[(place + distance) % len(TAB_ORDER)]
        return replace(self, tab=moved, pending=(), status=None)


@dataclass(frozen=True, slots=True)
class AddTask:
    """Create a task in the pane's project."""

    description: Description
    project: ProjectPath | None


@dataclass(frozen=True, slots=True)
class Retitle:
    """Give an existing task a new description."""

    task_id: TaskId
    description: Description


@dataclass(frozen=True, slots=True)
class Reproject:
    """Move a task to a project, creating that project if it is new."""

    task_id: TaskId
    project: ProjectPath | None


@dataclass(frozen=True, slots=True)
class Reschedule:
    """Give a task a deadline, or take its deadline away."""

    task_id: TaskId
    due: datetime | None


@dataclass(frozen=True, slots=True)
class RescheduleProject:
    """Give a project a deadline that everything beneath it inherits."""

    project: ProjectPath
    due: datetime | None


@dataclass(frozen=True, slots=True)
class Complete:
    """Mark a task done."""

    task_id: TaskId


@dataclass(frozen=True, slots=True)
class Reopen:
    """Put a finished task back on the list of work to do."""

    task_id: TaskId


@dataclass(frozen=True, slots=True)
class SwapPlaces:
    """Exchange where two tasks sit in the list."""

    first: TaskId
    second: TaskId


@dataclass(frozen=True, slots=True)
class SwapProjectPlaces:
    """Exchange where two projects sit among their neighbours."""

    first: ProjectPath
    second: ProjectPath


@dataclass(frozen=True, slots=True)
class StartClock:
    """Begin tracking time against a task."""

    task_id: TaskId


@dataclass(frozen=True, slots=True)
class StopClock:
    """Stop tracking time against a task."""

    task_id: TaskId


@dataclass(frozen=True, slots=True)
class MakeProject:
    """Record a project, whether or not anything is in it yet."""

    project: ProjectPath


@dataclass(frozen=True, slots=True)
class ForgetProject:
    """Remove a project that holds nothing."""

    project: ProjectPath


@dataclass(frozen=True, slots=True)
class RenameProject:
    """Move a project and everything beneath it."""

    old: ProjectPath
    new: ProjectPath


@dataclass(frozen=True, slots=True)
class Quit:
    """Leave, erasing the drawn region."""


@dataclass(frozen=True, slots=True)
class Added:
    """A task was created and is worth selecting."""

    task_id: TaskId


@dataclass(frozen=True, slots=True)
class Moved:
    """A number of tasks changed project."""

    count: int


@dataclass(frozen=True, slots=True)
class Did:
    """The command succeeded and had nothing else to report."""


Outcome = Added | Moved | Did


Command = (
    AddTask
    | Retitle
    | Reproject
    | Reschedule
    | RescheduleProject
    | Complete
    | Reopen
    | SwapPlaces
    | StartClock
    | StopClock
    | MakeProject
    | ForgetProject
    | RenameProject
    | SwapProjectPlaces
    | Quit
)
