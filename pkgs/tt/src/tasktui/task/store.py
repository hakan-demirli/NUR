"""The persistence boundary.

Every operation the interface can perform is one method here, so a backend can
be replaced without the interface noticing.  Implementations are responsible
for keeping at most one task active at a time.
"""

from __future__ import annotations

from dataclasses import replace
from datetime import datetime
from typing import Protocol

from tasktui.task.model import (
    Description,
    Done,
    Interval,
    Pending,
    ProjectPath,
    Running,
    Snapshot,
    Task,
    TaskId,
)


class StoreError(Exception):
    """A task operation could not be carried out."""


class UnknownTask(StoreError):
    """The referenced task does not exist."""

    def __init__(self, task_id: TaskId) -> None:
        super().__init__(f"no task with id {task_id}")
        self.task_id = task_id


class TaskAlreadyDone(StoreError):
    """The referenced task has already been completed."""

    def __init__(self, task_id: TaskId) -> None:
        super().__init__(f"task {task_id} is already done")
        self.task_id = task_id


class UnknownProject(StoreError):
    """The referenced project is not recorded."""

    def __init__(self, project: ProjectPath) -> None:
        super().__init__(f"no project {project}")
        self.project = project


class TaskNotDone(StoreError):
    """The referenced task was not finished, so it cannot be reopened."""

    def __init__(self, task_id: TaskId) -> None:
        super().__init__(f"task {task_id} is not done")
        self.task_id = task_id


class ProjectNotEmpty(StoreError):
    """The project still holds something, so it was not removed."""

    def __init__(self, project: ProjectPath, held: int) -> None:
        thing = "task" if held == 1 else "tasks"
        super().__init__(f"{project} still holds {held} {thing}")
        self.project = project


class NotRunning(StoreError):
    """A clock was stopped on a task that had none running."""

    def __init__(self, task_id: TaskId) -> None:
        super().__init__(f"no clock is running on task {task_id}")
        self.task_id = task_id


class TaskStore(Protocol):
    """Storage for tasks and the time spent on them."""

    def snapshot(self) -> Snapshot:
        """Read everything needed to draw a frame."""
        ...

    def add(
        self,
        description: Description,
        project: ProjectPath | None,
        now: datetime,
    ) -> TaskId:
        """Create a pending task and return its identifier."""
        ...

    def set_description(self, task_id: TaskId, description: Description) -> None:
        """Replace the description of an existing task."""
        ...

    def set_project(self, task_id: TaskId, project: ProjectPath | None) -> None:
        """Move a task to a project, or to no project at all.

        Projects exist only because tasks refer to them, so this is also how a
        new project comes into being.
        """
        ...

    def set_due(self, task_id: TaskId, due: datetime | None) -> None:
        """Give a task a deadline, or take its deadline away."""
        ...

    def set_project_due(self, project: ProjectPath, due: datetime | None) -> None:
        """Give a project a deadline that everything beneath it inherits."""
        ...

    def add_project(self, project: ProjectPath) -> None:
        """Record a project, whether or not anything is in it yet.

        A project outlives the tasks put in it, so finishing the last one
        leaves the project standing rather than making it disappear.  Every
        project enclosing it is recorded too, so each row of the tree is a
        project in its own right.
        """
        ...

    def swap_project_places(self, first: ProjectPath, second: ProjectPath) -> None:
        """Exchange where two projects sit among their neighbours.

        Everything beneath a project goes with it, since a project is placed
        relative to the one enclosing it.

        Raises:
            UnknownProject: if either project is not recorded.
        """
        ...

    def forget_project(self, project: ProjectPath) -> None:
        """Remove a project that holds nothing.

        Raises:
            ProjectNotEmpty: if any task or nested project is still in it.
        """
        ...

    def complete(self, task_id: TaskId, now: datetime) -> None:
        """Mark a task done, stopping its clock if it was running."""
        ...

    def reopen(self, task_id: TaskId) -> None:
        """Put a finished task back on the list of work to do.

        Raises:
            TaskNotDone: if the task was not finished in the first place.
        """
        ...

    def swap_places(self, first: TaskId, second: TaskId) -> None:
        """Exchange where two tasks sit in the list.

        Rearranging is by exchange rather than by naming a place, because the
        pane doing the asking may be showing only some of the tasks; the two
        it names are next to each other there, whatever lies between them in
        the list as a whole.

        Raises:
            UnknownTask: if either task is not there.
        """
        ...

    def start(self, task_id: TaskId, now: datetime) -> None:
        """Run a clock on a task.

        Any number of clocks may run at once.  Each records when it began, so
        stretches that overlap can be reconciled afterwards.
        """
        ...

    def stop(self, task_id: TaskId, now: datetime) -> None:
        """Stop the clock on a task and record the stretch it covered."""
        ...

    def rename_project(self, old: ProjectPath, new: ProjectPath) -> int:
        """Move ``old`` and everything beneath it to ``new``.

        Returns the number of tasks whose project changed.
        """
        ...

    def close(self) -> None:
        """Release any resources the store holds."""
        ...


class InMemoryStore:
    """A volatile store with the same semantics as the durable one."""

    def __init__(self) -> None:
        self._tasks: dict[TaskId, Task] = {}
        self._places: dict[TaskId, int] = {}
        self._projects: dict[ProjectPath, int] = {}
        self._project_due: dict[ProjectPath, datetime] = {}
        self._running: dict[TaskId, datetime] = {}
        self._closed: list[Interval] = []
        self._next_id = 1

    def snapshot(self) -> Snapshot:
        ordered = sorted(
            self._tasks.values(),
            key=lambda task: (self._places[task.id], task.id),
        )
        running = tuple(
            Running(task_id=task_id, since=since)
            for task_id, since in sorted(self._running.items())
        )
        open_now = tuple(
            Interval(task_id=clock.task_id, started_at=clock.since, stopped_at=None)
            for clock in running
        )
        return Snapshot(
            tasks=tuple(ordered),
            running=running,
            intervals=(*self._closed, *open_now),
            projects=dict(self._projects),
            project_due=dict(self._project_due),
        )

    def add(
        self,
        description: Description,
        project: ProjectPath | None,
        now: datetime,
    ) -> TaskId:
        task_id = TaskId(self._next_id)
        self._next_id += 1
        if project is not None:
            self._remember(project)
        self._places[task_id] = max(self._places.values(), default=0) + 1
        self._tasks[task_id] = Task(
            id=task_id,
            description=description,
            project=project,
            state=Pending(),
            created_at=now,
        )
        return task_id

    def swap_places(self, first: TaskId, second: TaskId) -> None:
        for wanted in (first, second):
            self._require(wanted)
        self._places[first], self._places[second] = (
            self._places[second],
            self._places[first],
        )

    def set_description(self, task_id: TaskId, description: Description) -> None:
        task = self._require(task_id)
        self._tasks[task_id] = replace(task, description=description)

    def set_project(self, task_id: TaskId, project: ProjectPath | None) -> None:
        task = self._require(task_id)
        if project is not None:
            self._remember(project)
        self._tasks[task_id] = replace(task, project=project)

    def add_project(self, project: ProjectPath) -> None:
        self._remember(project)

    def _remember(self, project: ProjectPath) -> None:
        """Record a project and everything enclosing it, each on the end.

        An enclosing project is recorded in its own right so that every row
        the tree shows is a project that can be arranged, renamed or dropped.
        """
        for step in (*project.ancestors(), project):
            if step not in self._projects:
                self._projects[step] = max(self._projects.values(), default=0) + 1

    def swap_project_places(self, first: ProjectPath, second: ProjectPath) -> None:
        for wanted in (first, second):
            if wanted not in self._projects:
                raise UnknownProject(wanted)
        self._projects[first], self._projects[second] = (
            self._projects[second],
            self._projects[first],
        )

    def forget_project(self, project: ProjectPath) -> None:
        held = sum(
            1
            for task in self._tasks.values()
            if task.project is not None and project.contains(task.project)
        )
        if held:
            raise ProjectNotEmpty(project, held)
        self._projects = {
            known: place
            for known, place in self._projects.items()
            if not project.contains(known)
        }
        self._project_due.pop(project, None)

    def set_due(self, task_id: TaskId, due: datetime | None) -> None:
        task = self._require(task_id)
        if not isinstance(task.state, Pending):
            raise TaskAlreadyDone(task_id)
        self._tasks[task_id] = replace(task, due=due)

    def set_project_due(self, project: ProjectPath, due: datetime | None) -> None:
        if due is None:
            self._project_due.pop(project, None)
        else:
            self._project_due[project] = due

    def complete(self, task_id: TaskId, now: datetime) -> None:
        task = self._require(task_id)
        if not isinstance(task.state, Pending):
            raise TaskAlreadyDone(task_id)
        if task_id in self._running:
            self.stop(task_id, now)
            task = self._require(task_id)
        self._tasks[task_id] = replace(task, state=Done(completed_at=now))

    def reopen(self, task_id: TaskId) -> None:
        task = self._require(task_id)
        if not isinstance(task.state, Done):
            raise TaskNotDone(task_id)
        self._tasks[task_id] = replace(task, state=Pending())

    def start(self, task_id: TaskId, now: datetime) -> None:
        task = self._require(task_id)
        if not isinstance(task.state, Pending):
            raise TaskAlreadyDone(task_id)
        self._running.setdefault(task_id, now)

    def stop(self, task_id: TaskId, now: datetime) -> None:
        since = self._running.pop(task_id, None)
        if since is None:
            raise NotRunning(task_id)
        self._closed.append(
            Interval(
                task_id=task_id,
                started_at=since,
                stopped_at=max(since, now),
            )
        )

    def rename_project(self, old: ProjectPath, new: ProjectPath) -> int:
        renamed = 0
        for task_id, task in list(self._tasks.items()):
            if task.project is None or not old.contains(task.project):
                continue
            self._tasks[task_id] = replace(task, project=task.project.rebased(old, new))
            renamed += 1
        for path in [path for path in self._project_due if old.contains(path)]:
            self._project_due[path.rebased(old, new)] = self._project_due.pop(path)
        self._projects = {
            (known.rebased(old, new) if old.contains(known) else known): place
            for known, place in self._projects.items()
        }
        self._remember(new)
        return renamed

    def close(self) -> None:
        self._tasks.clear()
        self._places.clear()
        self._projects.clear()
        self._project_due.clear()
        self._running.clear()
        self._closed.clear()

    def _require(self, task_id: TaskId) -> Task:
        task = self._tasks.get(task_id)
        if task is None:
            raise UnknownTask(task_id)
        return task
