"""What a key press means, separated from which key was pressed.

A binding is a run of keys rather than a single one, so ``g n`` can mean
something without ``g`` meaning anything on its own.  That only works if no
binding is the beginning of another, which is checked when bindings are built
rather than guessed at with a timer.

Only a pane's resting state is rebindable.  While a field is being typed into,
every printable character is text, so there is nothing there to bind.

The panes have separate vocabularies so that a configuration file cannot ask
for something a pane has no way to do; such a binding is refused when the file
is read rather than quietly ignored afterwards.
"""

from __future__ import annotations

from collections.abc import Mapping
from dataclasses import dataclass
from enum import Enum
from typing import Final

from tasktui.term.keys import (
    Char,
    KeyPress,
    parse_sequence,
    sequence_label,
)

Sequence = tuple[KeyPress, ...]


class ProjectAction(Enum):
    """Something that can be done while looking at the project tree."""

    MOVE_DOWN = "move_down"
    MOVE_UP = "move_up"
    PAGE_DOWN = "page_down"
    PAGE_UP = "page_up"
    MOVE_TOP = "move_top"
    MOVE_END = "move_end"
    SHIFT_DOWN = "shift_down"
    SHIFT_UP = "shift_up"
    OPEN = "open"
    NEW = "new"
    FORGET = "forget"
    FIND = "find"
    RENAME = "rename"
    SET_DUE = "set_due"
    NEXT_TAB = "next_tab"
    PREVIOUS_TAB = "previous_tab"
    QUIT = "quit"


class TaskAction(Enum):
    """Something that can be done while looking at a list of tasks."""

    MOVE_DOWN = "move_down"
    MOVE_UP = "move_up"
    PAGE_DOWN = "page_down"
    PAGE_UP = "page_up"
    MOVE_TOP = "move_top"
    MOVE_END = "move_end"
    SHIFT_DOWN = "shift_down"
    SHIFT_UP = "shift_up"
    BACK = "back"
    ADD = "add"
    EDIT = "edit"
    SET_PROJECT = "set_project"
    SET_DUE = "set_due"
    COMPLETE = "complete"
    TOGGLE_CLOCK = "toggle_clock"
    NEXT_TAB = "next_tab"
    PREVIOUS_TAB = "previous_tab"
    QUIT = "quit"


class AmbiguousBinding(ValueError):
    """One binding is the beginning of another, so neither could be reached."""


@dataclass(frozen=True, slots=True)
class Bound[A: Enum]:
    """The run of keys was a whole binding."""

    action: A


@dataclass(frozen=True, slots=True)
class Partial:
    """The run of keys begins a binding but is not one yet."""

    keys: Sequence


@dataclass(frozen=True, slots=True)
class Unbound:
    """The run of keys begins nothing."""


type Resolution[A: Enum] = Bound[A] | Partial | Unbound


@dataclass(frozen=True, slots=True)
class Bindings[A: Enum]:
    """The runs of keys understood by one pane."""

    entries: Mapping[Sequence, A]

    def resolve(self, pending: Sequence, press: KeyPress) -> Resolution[A]:
        """Fold one more key into whatever has been pressed so far."""
        attempt = (*pending, press)
        action = self.entries.get(attempt)
        if action is not None:
            return Bound(action)
        if any(keys[: len(attempt)] == attempt for keys in self.entries):
            return Partial(attempt)
        return Unbound()

    def keys_for(self, action: A) -> list[Sequence]:
        return [keys for keys, bound in self.entries.items() if bound == action]


def build[A: Enum](wanted: Mapping[A, tuple[str, ...]]) -> Bindings[A]:
    """Turn written bindings into a lookup, refusing ambiguous ones.

    Raises:
        AmbiguousBinding: if one run of keys begins another.
        UnknownKey: if a run names a key that cannot be pressed.
    """
    entries: dict[Sequence, A] = {}
    for action, written in wanted.items():
        for text in written:
            entries[parse_sequence(text)] = action
    for keys in entries:
        for other in entries:
            if other != keys and other[: len(keys)] == keys:
                raise AmbiguousBinding(
                    f"{sequence_label(keys)!r} is also the start of "
                    f"{sequence_label(other)!r}, so it could never be pressed"
                )
    return Bindings(entries)


@dataclass(frozen=True, slots=True)
class Keymap:
    """Which keys do what, in each pane."""

    projects: Bindings[ProjectAction]
    tasks: Bindings[TaskAction]


# The defaults follow Helix, since that is where the muscle memory comes from:
# `g g` and `g e` for the ends, `g n` and `g p` for the next and previous of
# whatever is in the strip along the top, `c` to change, `o` to open a new one
# below, and ctrl-u and ctrl-d for half a screen.  Escape is deliberately not
# a way out: in Helix it collapses a selection and is pressed constantly, so
# leaving on it would be an accident waiting to happen.
DEFAULT_PROJECT_KEYS: Final[dict[ProjectAction, tuple[str, ...]]] = {
    ProjectAction.MOVE_DOWN: ("j", "down"),
    ProjectAction.MOVE_UP: ("k", "up"),
    ProjectAction.PAGE_DOWN: ("ctrl-d", "pagedown"),
    ProjectAction.PAGE_UP: ("ctrl-u", "pageup"),
    ProjectAction.MOVE_TOP: ("g g", "home"),
    ProjectAction.MOVE_END: ("g e", "end"),
    ProjectAction.SHIFT_DOWN: ("J",),
    ProjectAction.SHIFT_UP: ("K",),
    ProjectAction.NEXT_TAB: ("g n",),
    ProjectAction.PREVIOUS_TAB: ("g p",),
    ProjectAction.OPEN: ("l", "right", "enter"),
    ProjectAction.NEW: ("o",),
    ProjectAction.FORGET: ("d",),
    ProjectAction.FIND: ("f",),
    ProjectAction.RENAME: ("r",),
    ProjectAction.SET_DUE: ("D",),
    ProjectAction.QUIT: ("q",),
}

DEFAULT_TASK_KEYS: Final[dict[TaskAction, tuple[str, ...]]] = {
    TaskAction.MOVE_DOWN: ("j", "down"),
    TaskAction.MOVE_UP: ("k", "up"),
    TaskAction.PAGE_DOWN: ("ctrl-d", "pagedown"),
    TaskAction.PAGE_UP: ("ctrl-u", "pageup"),
    TaskAction.MOVE_TOP: ("g g", "home"),
    TaskAction.MOVE_END: ("g e", "end"),
    TaskAction.SHIFT_DOWN: ("J",),
    TaskAction.SHIFT_UP: ("K",),
    TaskAction.NEXT_TAB: ("g n",),
    TaskAction.PREVIOUS_TAB: ("g p",),
    TaskAction.BACK: ("h", "left"),
    TaskAction.ADD: ("o",),
    TaskAction.EDIT: ("c",),
    TaskAction.SET_PROJECT: ("p",),
    TaskAction.SET_DUE: ("D",),
    TaskAction.COMPLETE: ("d",),
    TaskAction.TOGGLE_CLOCK: ("s",),
    TaskAction.QUIT: ("q",),
}


def default_keymap() -> Keymap:
    """The bindings used when nothing has been configured."""
    return Keymap(
        projects=build(DEFAULT_PROJECT_KEYS),
        tasks=build(DEFAULT_TASK_KEYS),
    )


@dataclass(frozen=True, slots=True)
class Hint:
    """One reminder, and whether it may be left out when space runs short."""

    text: str
    essential: bool


@dataclass(frozen=True, slots=True)
class _Entry[A: Enum]:
    label: str
    actions: tuple[A, ...]
    essential: bool = False


PROJECT_HINTS: Final[tuple[_Entry[ProjectAction], ...]] = (
    _Entry("move", (ProjectAction.MOVE_DOWN, ProjectAction.MOVE_UP), essential=True),
    _Entry("tab", (ProjectAction.NEXT_TAB,), essential=True),
    _Entry("open", (ProjectAction.OPEN,)),
    _Entry("new", (ProjectAction.NEW,), essential=True),
    _Entry("order", (ProjectAction.SHIFT_DOWN, ProjectAction.SHIFT_UP)),
    _Entry("find", (ProjectAction.FIND,)),
    _Entry("rename", (ProjectAction.RENAME,)),
    _Entry("due", (ProjectAction.SET_DUE,)),
    _Entry("forget", (ProjectAction.FORGET,)),
    _Entry("quit", (ProjectAction.QUIT,), essential=True),
)

TASK_HINTS: Final[tuple[_Entry[TaskAction], ...]] = (
    _Entry("move", (TaskAction.MOVE_DOWN, TaskAction.MOVE_UP), essential=True),
    _Entry("tab", (TaskAction.NEXT_TAB,), essential=True),
    _Entry("back", (TaskAction.BACK,), essential=True),
    _Entry("add", (TaskAction.ADD,)),
    _Entry("order", (TaskAction.SHIFT_DOWN, TaskAction.SHIFT_UP)),
    _Entry("edit", (TaskAction.EDIT,)),
    _Entry("project", (TaskAction.SET_PROJECT,)),
    _Entry("due", (TaskAction.SET_DUE,)),
    _Entry("done", (TaskAction.COMPLETE,)),
    _Entry("clock", (TaskAction.TOGGLE_CLOCK,)),
    _Entry("quit", (TaskAction.QUIT,), essential=True),
)


def _handiest[A: Enum](bindings: Bindings[A], action: A) -> str | None:
    """The keys a person is most likely to reach for, or nothing if unbound."""
    bound = bindings.keys_for(action)
    if not bound:
        return None
    plain = sorted(
        (keys for keys in bound if all(isinstance(press, Char) for press in keys)),
        key=len,
    )
    return sequence_label(plain[0] if plain else bound[0])


def hint_parts[A: Enum](
    bindings: Bindings[A],
    entries: tuple[_Entry[A], ...],
) -> list[Hint]:
    """Reminders of the bindings actually in force.

    They are returned one by one, and marked, so that a narrow terminal can be
    given fewer of them rather than half of the last one.
    """
    parts: list[Hint] = []
    for entry in entries:
        keys = [
            name
            for name in (_handiest(bindings, action) for action in entry.actions)
            if name
        ]
        if keys:
            parts.append(Hint(f"{'/'.join(keys)} {entry.label}", entry.essential))
    return parts


ON_A_PROJECT: Final = frozenset(
    {
        ProjectAction.RENAME,
        ProjectAction.SET_DUE,
        ProjectAction.FORGET,
        ProjectAction.SHIFT_DOWN,
        ProjectAction.SHIFT_UP,
    }
)


def project_hints(keymap: Keymap, *, on_project: bool = True) -> list[Hint]:
    """Reminders for the project tree.

    Rows such as "all tasks" stand for a selection rather than for a project,
    so nothing that acts on a project is offered while one is selected.
    """
    entries = (
        PROJECT_HINTS
        if on_project
        else tuple(
            entry
            for entry in PROJECT_HINTS
            if not ON_A_PROJECT.intersection(entry.actions)
        )
    )
    return hint_parts(keymap.projects, entries)


IN_A_PROJECT: Final = frozenset(
    {
        TaskAction.BACK,
        TaskAction.ADD,
        TaskAction.SHIFT_DOWN,
        TaskAction.SHIFT_UP,
    }
)


def task_hints(keymap: Keymap, *, nested: bool = True) -> list[Hint]:
    """Reminders for a task pane.

    A pane reached by drilling into a project can be left again, is somewhere
    a new task can be put, and is a list that can be put in an order.  The
    agenda is none of those: it is a view of what already has a deadline, in
    the order the deadlines fall, so it is not told about any of them.
    """
    entries = (
        TASK_HINTS
        if nested
        else tuple(
            entry
            for entry in TASK_HINTS
            if not IN_A_PROJECT.intersection(entry.actions)
        )
    )
    return hint_parts(keymap.tasks, entries)
