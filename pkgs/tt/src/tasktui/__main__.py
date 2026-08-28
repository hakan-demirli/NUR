"""Command line entry point and the event loop."""

from __future__ import annotations

import argparse
import sys
from collections.abc import Callable
from dataclasses import replace
from datetime import UTC, datetime
from pathlib import Path

from rich.console import Console

from tasktui.config import (
    ConfigError,
    Settings,
    as_toml,
    default_config_path,
    load,
)
from tasktui.task.duckdb_store import DuckdbStore
from tasktui.task.model import (
    InvalidProjectPath,
    NoProject,
    ProjectFilter,
    ProjectPath,
    UnderProject,
)
from tasktui.task.store import StoreError, TaskStore
from tasktui.term.viewport import InlineViewport, NotATerminal, Resize
from tasktui.ui.action import Keymap
from tasktui.ui.state import (
    Added,
    AddTask,
    Command,
    Complete,
    Did,
    ForgetProject,
    MakeProject,
    Moved,
    Outcome,
    Problem,
    Quit,
    RenameProject,
    Reopen,
    Reproject,
    Reschedule,
    RescheduleProject,
    Retitle,
    StartClock,
    State,
    StopClock,
    SwapPlaces,
    SwapProjectPlaces,
)
from tasktui.ui.theme import Look
from tasktui.ui.update import describe, focus, initial_state, reanchor, update
from tasktui.ui.view import render

Clock = Callable[[], datetime]


def parse_arguments(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        prog="tt",
        description="An inline terminal UI for task tracking.",
    )
    parser.add_argument(
        "--config",
        type=Path,
        default=default_config_path(),
        metavar="PATH",
        help="configuration file (default: %(default)s)",
    )
    parser.add_argument(
        "--db",
        type=Path,
        default=None,
        metavar="PATH",
        help="database file, overriding the configuration",
    )
    parser.add_argument(
        "--dump-config",
        action="store_true",
        help="print the settings in force as a configuration file, then exit",
    )
    parser.add_argument(
        "--memory",
        action="store_true",
        help="use a throwaway database that is discarded on exit",
    )
    opening = parser.add_mutually_exclusive_group()
    opening.add_argument(
        "--project",
        metavar="PATH",
        help="open the task list for a project instead of the project tree",
    )
    opening.add_argument(
        "--no-project",
        action="store_true",
        help="open the task list for tasks that have no project",
    )
    return parser.parse_args(argv)


def opening_scope(arguments: argparse.Namespace) -> ProjectFilter | None:
    """Which pane to start on, read from the command line."""
    if arguments.no_project:
        return NoProject()
    if arguments.project is None:
        return None
    return UnderProject(ProjectPath.parse(arguments.project))


def carry_out(command: Command, store: TaskStore, now: datetime) -> Outcome:
    """Perform one command and report anything the interface should react to."""
    match command:
        case AddTask(description, project):
            return Added(store.add(description, project, now))
        case Retitle(task_id, description):
            store.set_description(task_id, description)
        case Reproject(task_id, project):
            store.set_project(task_id, project)
        case Reschedule(task_id, due):
            store.set_due(task_id, due)
        case RescheduleProject(project, due):
            store.set_project_due(project, due)
        case Complete(task_id):
            store.complete(task_id, now)
        case Reopen(task_id):
            store.reopen(task_id)
        case SwapPlaces(first, second):
            store.swap_places(first, second)
        case SwapProjectPlaces(first, second):
            store.swap_project_places(first, second)
        case StartClock(task_id):
            store.start(task_id, now)
        case StopClock(task_id):
            store.stop(task_id, now)
        case MakeProject(project):
            store.add_project(project)
        case ForgetProject(project):
            store.forget_project(project)
        case RenameProject(old, new):
            return Moved(store.rename_project(old, new))
        case Quit():
            pass
    return Did()


def apply(command: Command, store: TaskStore, state: State, clock: Clock) -> State:
    """Perform a command and fold its result back into the screen."""
    try:
        outcome = carry_out(command, store, clock())
    except StoreError as error:
        return replace(state, status=Problem(str(error)))
    settled = replace(state, status=describe(command, outcome))
    if isinstance(outcome, Added):
        return focus(settled, outcome.task_id)
    return settled


def run(
    viewport: InlineViewport,
    store: TaskStore,
    scope: ProjectFilter | None,
    clock: Clock,
    keymap: Keymap,
    look: Look,
) -> None:
    """Draw, wait, react, until the user leaves."""
    snapshot = store.snapshot()
    state = reanchor(initial_state(scope), snapshot)

    def draw() -> None:
        frame = render(
            state, snapshot, clock(), viewport.height, viewport.width, keymap, look
        )
        viewport.render(frame)

    draw()
    while True:
        event = viewport.next_event()
        if event is None:
            return
        if isinstance(event, Resize):
            draw()
            continue

        # A paging key travels half of whatever the pane can show.
        page = max(1, viewport.height // 2)
        state, commands = update(event, state, snapshot, keymap, clock(), page)
        for command in commands:
            if isinstance(command, Quit):
                return
            state = apply(command, store, state, clock)

        snapshot = store.snapshot()
        state = reanchor(state, snapshot)
        draw()


def _clock() -> Clock:
    """A clock reporting the moment in the machine's own zone.

    Deadlines are said in local terms, so which day it is has to be decided
    where the person is; the store still keeps every instant as UTC.
    """
    return lambda: datetime.now(UTC).astimezone()


def open_store(arguments: argparse.Namespace, settings: Settings) -> DuckdbStore:
    if arguments.memory:
        return DuckdbStore.in_memory()
    return DuckdbStore.open(arguments.db or settings.database)


def main(argv: list[str] | None = None) -> int:
    """Run the interface, returning a process exit code."""
    arguments = parse_arguments(argv)
    try:
        settings = load(arguments.config)
        scope = opening_scope(arguments)
    except (ConfigError, InvalidProjectPath) as error:
        print(f"tt: {error}", file=sys.stderr)
        return 2

    if arguments.dump_config:
        print(as_toml(replace(settings, database=arguments.db or settings.database)))
        return 0

    try:
        store = open_store(arguments, settings)
    except StoreError as error:
        print(f"tt: {error}", file=sys.stderr)
        return 1

    console = Console(theme=settings.theme)
    try:
        with InlineViewport(console) as viewport:
            run(
                viewport,
                store,
                scope,
                _clock(),
                settings.keymap,
                settings.look,
            )
    except NotATerminal as error:
        print(f"tt: {error}", file=sys.stderr)
        return 2
    finally:
        store.close()
    return 0


if __name__ == "__main__":
    sys.exit(main())
