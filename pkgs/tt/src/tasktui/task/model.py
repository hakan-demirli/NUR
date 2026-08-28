"""The task domain.

Values in this module are only constructible in states the storage layer also
permits, so that a task loaded from the database and a task built in a test are
subject to the same invariants.
"""

from __future__ import annotations

from collections.abc import Iterable, Mapping
from dataclasses import dataclass, field
from datetime import datetime, timedelta
from typing import Final, NewType, Self

TaskId = NewType("TaskId", int)

PROJECT_SEPARATOR: Final = "."


class InvalidProjectPath(ValueError):
    """Raised when text cannot be read as a project path."""


class InvalidDescription(ValueError):
    """Raised when text cannot be read as a task description."""


@dataclass(frozen=True, slots=True)
class Description:
    """Non-blank text describing a task.

    Construct with :meth:`parse`; the constructor trusts its argument.
    """

    text: str

    @classmethod
    def parse(cls, text: str) -> Self:
        """Read ``text`` as a description.

        Raises:
            InvalidDescription: if the text is blank, or holds a character
                that cannot be drawn.  A task occupies one row, so a newline
                would push the frame past the height it was given.
        """
        stripped = text.strip()
        if not stripped:
            raise InvalidDescription("a description cannot be empty")
        if not stripped.isprintable():
            raise InvalidDescription(
                f"{stripped!r} contains a character that cannot be shown",
            )
        return cls(stripped)

    def __str__(self) -> str:
        return self.text


@dataclass(frozen=True, slots=True)
class ProjectPath:
    """A dotted hierarchy of project names, such as ``home.garden``.

    Construct with :meth:`parse`; the constructor trusts its argument.
    """

    segments: tuple[str, ...]

    @classmethod
    def parse(cls, text: str) -> Self:
        """Read ``text`` as a project path.

        Segments may contain spaces, so "Home Renovation.Kitchen" names a
        project beneath another.  Space around a separator is not part of a
        name and is dropped, which keeps two spellings of the same project
        from both existing.

        Raises:
            InvalidProjectPath: if the text is empty, if any segment is empty,
                or if a segment holds a character that cannot be drawn.
        """
        if not text.strip():
            raise InvalidProjectPath("a project path cannot be empty")
        segments = tuple(part.strip() for part in text.split(PROJECT_SEPARATOR))
        for segment in segments:
            if not segment:
                raise InvalidProjectPath(f"empty segment in project path {text!r}")
            if not segment.isprintable():
                raise InvalidProjectPath(
                    f"project segment {segment!r} contains a character "
                    "that cannot be shown",
                )
        return cls(segments)

    def __str__(self) -> str:
        return PROJECT_SEPARATOR.join(self.segments)

    @property
    def name(self) -> str:
        """The last segment, which is how the path is labelled in a tree."""
        return self.segments[-1]

    @property
    def depth(self) -> int:
        """How many ancestors the path has."""
        return len(self.segments) - 1

    @property
    def parent(self) -> ProjectPath | None:
        """The enclosing path, or ``None`` for a top level project."""
        if len(self.segments) == 1:
            return None
        return ProjectPath(self.segments[:-1])

    def ancestors(self) -> tuple[ProjectPath, ...]:
        """Every enclosing path, outermost first."""
        return tuple(
            ProjectPath(self.segments[:length])
            for length in range(1, len(self.segments))
        )

    def contains(self, other: ProjectPath) -> bool:
        """Whether ``other`` is this path or lies beneath it."""
        return other.segments[: len(self.segments)] == self.segments

    def rebased(self, old_root: ProjectPath, new_root: ProjectPath) -> ProjectPath:
        """Move this path from beneath ``old_root`` to beneath ``new_root``.

        The segments below the old root are preserved, so renaming ``home`` to
        ``house`` turns ``home.garden`` into ``house.garden``.
        """
        if not old_root.contains(self):
            return self
        return ProjectPath(new_root.segments + self.segments[len(old_root.segments) :])


@dataclass(frozen=True, slots=True)
class AllProjects:
    """Every task, regardless of project."""


@dataclass(frozen=True, slots=True)
class NoProject:
    """Only tasks that have no project."""


@dataclass(frozen=True, slots=True)
class UnderProject:
    """Only tasks at a project path or beneath it."""

    path: ProjectPath


ProjectScope = NoProject | UnderProject
ProjectFilter = AllProjects | ProjectScope


ALL_TASKS_LABEL: Final = "all tasks"
NO_PROJECT_LABEL: Final = "(no project)"


def scope_label(scope: ProjectFilter) -> str:
    """A human readable name for a project filter."""
    match scope:
        case AllProjects():
            return ALL_TASKS_LABEL
        case NoProject():
            return NO_PROJECT_LABEL
        case UnderProject(path):
            return str(path)


@dataclass(frozen=True, slots=True)
class Pending:
    """The task is still to be done."""


@dataclass(frozen=True, slots=True)
class Done:
    """The task was completed at a known time."""

    completed_at: datetime


TaskState = Pending | Done


@dataclass(frozen=True, slots=True)
class Task:
    """A single unit of work."""

    id: TaskId
    description: Description
    project: ProjectPath | None
    state: TaskState
    created_at: datetime
    due: datetime | None = None

    def matches(self, project_filter: ProjectFilter) -> bool:
        """Whether this task belongs in a pane showing ``project_filter``."""
        match project_filter:
            case AllProjects():
                return True
            case NoProject():
                return self.project is None
            case UnderProject(path):
                return self.project is not None and path.contains(self.project)


@dataclass(frozen=True, slots=True)
class Running:
    """A clock currently running on a task.

    Any number may run at once.  Each one is a stretch of time with a start
    and, once stopped, an end, so overlapping stretches can be reconciled
    rather than having to be forbidden.
    """

    task_id: TaskId
    since: datetime


@dataclass(frozen=True, slots=True)
class Interval:
    """A stretch of time spent on a task."""

    task_id: TaskId
    started_at: datetime
    stopped_at: datetime | None

    def ended_by(self, now: datetime) -> datetime:
        """When it finished, treating a running clock as ending now."""
        return (
            self.stopped_at
            if self.stopped_at is not None
            else max(self.started_at, now)
        )

    def length(self, now: datetime) -> timedelta:
        return self.ended_by(now) - self.started_at


def merged_duration(intervals: Iterable[Interval], now: datetime) -> timedelta:
    """How much time actually passed, counting overlaps only once.

    Summing intervals would say two hours were spent in an hour during which
    two clocks ran.  Merging the stretches first gives the time that really
    went by.
    """
    ordered = sorted(
        ((span.started_at, span.ended_by(now)) for span in intervals),
        key=lambda span: span[0],
    )
    total = timedelta()
    current: tuple[datetime, datetime] | None = None
    for started, ended in ordered:
        if current is None:
            current = (started, ended)
        elif started <= current[1]:
            current = (current[0], max(current[1], ended))
        else:
            total += current[1] - current[0]
            current = (started, ended)
    if current is not None:
        total += current[1] - current[0]
    return total


@dataclass(frozen=True, slots=True)
class Snapshot:
    """Everything the interface needs to draw one frame."""

    tasks: tuple[Task, ...]
    running: tuple[Running, ...] = ()
    intervals: tuple[Interval, ...] = ()
    projects: Mapping[ProjectPath, int] = field(default_factory=dict)
    project_due: Mapping[ProjectPath, datetime] = field(default_factory=dict)

    def is_running(self, task_id: TaskId) -> bool:
        return any(clock.task_id == task_id for clock in self.running)

    def spent_on(self, tasks: Iterable[Task], now: datetime) -> timedelta:
        """Time that really went by across a group, overlaps counted once."""
        wanted = {task.id for task in tasks}
        return merged_duration(
            (span for span in self.intervals if span.task_id in wanted), now
        )

    def due_for(self, task: Task) -> datetime | None:
        """When a task is wanted by.

        A task's own deadline wins.  Failing that it takes the deadline of the
        closest enclosing project that has one, so a deadline set on a project
        reaches everything beneath it.
        """
        if task.due is not None:
            return task.due
        return self.project_deadline(task.project)

    def project_deadline(self, project: ProjectPath | None) -> datetime | None:
        """The deadline a project inherits, closest ancestor first."""
        if project is None:
            return None
        for candidate in (project, *reversed(project.ancestors())):
            deadline = self.project_due.get(candidate)
            if deadline is not None:
                return deadline
        return None

    def pending(self) -> tuple[Task, ...]:
        """Tasks that are not yet done, oldest first."""
        return tuple(task for task in self.tasks if isinstance(task.state, Pending))

    def find(self, task_id: TaskId) -> Task | None:
        """The task with this identifier, if it still exists."""
        return next((task for task in self.tasks if task.id == task_id), None)

    def elapsed(self, task: Task, now: datetime) -> timedelta:
        """Time spent on one task, including any clock still running on it.

        Stretches on a single task cannot overlap each other, so this is a
        plain sum; only totals across several tasks need merging.
        """
        return sum(
            (span.length(now) for span in self.intervals if span.task_id == task.id),
            timedelta(),
        )


@dataclass(frozen=True, slots=True)
class ProjectSummary:
    """One row of the project pane."""

    scope: ProjectFilter
    pending: int
    tracked: timedelta
    due: datetime | None = None

    @property
    def depth(self) -> int:
        """Indentation level of this row within the project tree."""
        match self.scope:
            case AllProjects() | NoProject():
                return 0
            case UnderProject(path):
                return path.depth

    @property
    def label(self) -> str:
        """The text shown for this row, without indentation."""
        return row_label(self.scope)


def row_label(scope: ProjectFilter) -> str:
    """How a tree row is written, which for a project is its last segment.

    The enclosing projects are already shown by the rows above it, so only the
    name it adds is repeated here.
    """
    match scope:
        case AllProjects():
            return ALL_TASKS_LABEL
        case NoProject():
            return NO_PROJECT_LABEL
        case UnderProject(path):
            return path.name


def summary_scopes(snapshot: Snapshot) -> tuple[ProjectFilter, ...]:
    """The rows of the project tree, in order.

    Every project holding tasks appears, along with any ancestor that would
    otherwise be missing, so the tree never has a gap.  A row covering every
    task always leads, so the tree is never empty and there is always a way
    into a task list.

    Only the order is worked out here, which is all that moving a cursor
    needs; what each row holds is counted by :func:`summarize`.
    """
    # Finished work stays on the task panes, so every row that could hold it
    # has to stay on the tree as well; a project or a scope that vanished the
    # moment its last task was done would put that record out of reach.
    named = {
        *snapshot.projects,
        *snapshot.project_due,
        *(task.project for task in snapshot.tasks if task.project is not None),
    }
    paths: set[ProjectPath] = set()
    for path in named:
        paths.add(path)
        paths.update(path.ancestors())

    def beneath(path: ProjectPath) -> tuple[tuple[int, str], ...]:
        """Where a path falls: under its parent, at the place it was given.

        Sorting on the places of the projects enclosing a path and then on its
        own puts a project after its parent and carries everything beneath it
        along, so moving one project moves its whole branch.  A project with
        no place recorded sorts after those that have one, by name, which
        leaves a tree nobody has arranged in alphabetical order.
        """
        unplaced = len(snapshot.projects)
        return tuple(
            (snapshot.projects.get(step, unplaced), str(step))
            for step in (*path.ancestors(), path)
        )

    scopes: list[ProjectFilter] = [AllProjects()]
    if any(task.project is None for task in snapshot.tasks):
        scopes.append(NoProject())
    scopes.extend(UnderProject(path) for path in sorted(paths, key=beneath))
    return tuple(scopes)


def summarize(snapshot: Snapshot, now: datetime) -> tuple[ProjectSummary, ...]:
    """Count what sits under each row of the project tree.

    Counts and time include everything beneath a path, not just the tasks
    sitting directly on it.  Time is merged rather than added up, so an hour
    with two clocks running counts as the hour it really was.
    """
    pending = snapshot.pending()

    def covered(scope: ProjectFilter) -> list[Task]:
        return [task for task in pending if task.matches(scope)]

    return tuple(
        ProjectSummary(
            scope=scope,
            pending=len(covered(scope)),
            tracked=snapshot.spent_on(covered(scope), now),
            due=(
                snapshot.project_due.get(scope.path)
                if isinstance(scope, UnderProject)
                else None
            ),
        )
        for scope in summary_scopes(snapshot)
    )
