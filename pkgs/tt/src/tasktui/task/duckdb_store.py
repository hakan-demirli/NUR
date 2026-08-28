"""A durable :class:`~tasktui.task.store.TaskStore` backed by DuckDB.

The schema carries the same invariants as the domain model.  A done task
cannot lack a completion time, an interval cannot be left open, and two tasks
cannot be active at once, because the engine refuses to record any of them.
"""

from __future__ import annotations

from collections.abc import Iterator, Sequence
from contextlib import contextmanager
from datetime import datetime
from pathlib import Path
from typing import Any, Final

import duckdb

from tasktui.task.model import (
    PROJECT_SEPARATOR,
    Description,
    Done,
    Interval,
    Pending,
    ProjectPath,
    Running,
    Snapshot,
    Task,
    TaskId,
    TaskState,
)
from tasktui.task.store import (
    NotRunning,
    ProjectNotEmpty,
    StoreError,
    TaskAlreadyDone,
    TaskNotDone,
    UnknownProject,
    UnknownTask,
)

MIGRATIONS: Final[tuple[tuple[str, ...], ...]] = (
    (
        """
        CREATE SEQUENCE task_id_seq START 1
        """,
        """
        CREATE TABLE task (
          id           BIGINT PRIMARY KEY DEFAULT nextval('task_id_seq'),
          description  VARCHAR NOT NULL CHECK (description <> ''),
          project      VARCHAR,
          status       VARCHAR NOT NULL CHECK (status IN ('pending', 'done')),
          created_at   TIMESTAMPTZ NOT NULL,
          completed_at TIMESTAMPTZ,
          CHECK ((status = 'done') = (completed_at IS NOT NULL))
        )
        """,
        """
        CREATE SEQUENCE work_interval_id_seq START 1
        """,
        """
        CREATE TABLE work_interval (
          id         BIGINT PRIMARY KEY DEFAULT nextval('work_interval_id_seq'),
          task_id    BIGINT NOT NULL REFERENCES task(id),
          started_at TIMESTAMPTZ NOT NULL,
          stopped_at TIMESTAMPTZ NOT NULL,
          CHECK (stopped_at >= started_at)
        )
        """,
        """
        CREATE TABLE active (
          task_id    BIGINT PRIMARY KEY REFERENCES task(id),
          started_at TIMESTAMPTZ NOT NULL,
          singleton  BOOLEAN NOT NULL DEFAULT true UNIQUE CHECK (singleton)
        )
        """,
    ),
    (
        """
        ALTER TABLE task ADD COLUMN due TIMESTAMPTZ
        """,
        """
        CREATE TABLE project_deadline (
          project VARCHAR PRIMARY KEY CHECK (project <> ''),
          due     TIMESTAMPTZ NOT NULL
        )
        """,
    ),
    (
        # Clocks may now run on several tasks at once. Every stretch keeps its
        # own start and end, so overlapping ones are reconciled when a total
        # is worked out rather than being prevented from happening.
        """
        CREATE TABLE running (
          task_id    BIGINT PRIMARY KEY REFERENCES task(id),
          started_at TIMESTAMPTZ NOT NULL
        )
        """,
        """
        INSERT INTO running SELECT task_id, started_at FROM active
        """,
        """
        DROP TABLE active
        """,
    ),
    (
        # A project is a thing in its own right, not a side effect of the tasks
        # that mention one, so it can be made before there is anything in it
        # and outlives the last task being finished.
        """
        CREATE TABLE project (
          path VARCHAR PRIMARY KEY CHECK (path <> '')
        )
        """,
        """
        INSERT INTO project
        SELECT DISTINCT project FROM task WHERE project IS NOT NULL
        """,
        """
        INSERT INTO project
        SELECT project FROM project_deadline
        WHERE project NOT IN (SELECT path FROM project)
        """,
    ),
    (
        # A list has an order of its own, arranged by whoever keeps it, which
        # is not the order the tasks happened to be written down in.  It lives
        # in its own table because other tables point at `task`, and duckdb
        # will not alter a table that is depended on.  Existing rows take
        # their place from their identifier, so a list already kept reads
        # exactly as it did before.
        """
        CREATE TABLE task_place (
          task_id BIGINT PRIMARY KEY REFERENCES task(id),
          place BIGINT NOT NULL
        )
        """,
        """
        INSERT INTO task_place SELECT id, id FROM task
        """,
    ),
    (
        # The tree has an order of its own too.  The table is built afresh and
        # swapped in rather than altered in place: duckdb will not rebuild the
        # key of a table that has updates outstanding, and everything here
        # happens in one transaction.  Nothing points at `project`, so it can
        # be dropped once its contents have been carried across.
        """
        CREATE TABLE project_ordered (
          path VARCHAR PRIMARY KEY CHECK (path <> ''),
          place BIGINT NOT NULL
        )
        """,
        # Every enclosing project comes across as a row in its own right, so
        # that each row the tree shows is something that can be arranged and
        # renamed.  Places start out alphabetical, so a tree nobody has
        # arranged reads exactly as it did before.
        f"""
        INSERT INTO project_ordered (path, place)
        SELECT path, row_number() OVER (ORDER BY path) FROM (
          SELECT path FROM project
          UNION
          SELECT array_to_string(parts[1:depth], '{PROJECT_SEPARATOR}')
          FROM (
            SELECT string_split(path, '{PROJECT_SEPARATOR}') AS parts FROM project
          ) AS split, unnest(range(1, len(parts))) AS enclosing(depth)
        )
        """,
        """
        DROP TABLE project
        """,
        """
        ALTER TABLE project_ordered RENAME TO project
        """,
    ),
)

# The join is outer, and falls back to the identifier, so that a task could
# never be hidden by having no place recorded for it.
_TASKS_QUERY: Final = """
    SELECT
      task.id, task.description, task.project,
      task.status, task.created_at, task.completed_at, task.due
    FROM task LEFT JOIN task_place ON task_place.task_id = task.id
    ORDER BY coalesce(task_place.place, task.id), task.id
"""

# Closed stretches and running ones are read as one list, a running one having
# no end yet, so that time is worked out from stretches alone.
_INTERVALS_QUERY: Final = """
    SELECT task_id, started_at, stopped_at FROM work_interval
    UNION ALL
    SELECT task_id, started_at, NULL FROM running
    ORDER BY started_at
"""


class DatabaseUnavailable(StoreError):
    """The database file could not be opened.

    DuckDB allows a single writing process per file, so this usually means
    another copy of the interface is already running.
    """


class DuckdbStore:
    """Tasks persisted in a DuckDB database."""

    def __init__(self, connection: duckdb.DuckDBPyConnection) -> None:
        self._connection = connection
        # Instants are stored as UTC; reading them back in the session's local
        # zone would be the same instant but would vary by machine.
        self._connection.execute("SET TimeZone = 'UTC'")
        self._migrate()

    @classmethod
    def open(cls, database: Path) -> DuckdbStore:
        """Open, creating the file and its parent directory if needed."""
        database.parent.mkdir(parents=True, exist_ok=True)
        try:
            connection = duckdb.connect(str(database))
        except duckdb.Error as error:
            raise DatabaseUnavailable(
                f"cannot open {database}: {error}",
            ) from error
        return cls(connection)

    @classmethod
    def in_memory(cls) -> DuckdbStore:
        """Open a throwaway database that lives only as long as the process."""
        return cls(duckdb.connect(":memory:"))

    def close(self) -> None:
        self._connection.close()

    def snapshot(self) -> Snapshot:
        rows = self._connection.execute(_TASKS_QUERY).fetchall()
        tasks = tuple(_read_task(row) for row in rows)
        running = tuple(
            Running(task_id=TaskId(task_id), since=since)
            for task_id, since in self._connection.execute(
                "SELECT task_id, started_at FROM running ORDER BY task_id",
            ).fetchall()
        )
        intervals = tuple(
            Interval(
                task_id=TaskId(task_id),
                started_at=started_at,
                stopped_at=stopped_at,
            )
            for task_id, started_at, stopped_at in self._connection.execute(
                _INTERVALS_QUERY,
            ).fetchall()
        )
        projects = {
            ProjectPath.parse(path): place
            for path, place in self._connection.execute(
                "SELECT path, place FROM project",
            ).fetchall()
        }
        deadlines = {
            ProjectPath.parse(project): due
            for project, due in self._connection.execute(
                "SELECT project, due FROM project_deadline",
            ).fetchall()
        }
        return Snapshot(
            tasks=tasks,
            running=running,
            intervals=intervals,
            projects=projects,
            project_due=deadlines,
        )

    def add(
        self,
        description: Description,
        project: ProjectPath | None,
        now: datetime,
    ) -> TaskId:
        with self._transaction():
            if project is not None:
                self._remember(project)
            rows = self._execute(
                """
                INSERT INTO task (description, project, status, created_at)
                VALUES (?, ?, 'pending', ?)
                RETURNING id
                """,
                (description.text, None if project is None else str(project), now),
            )
            task_id = TaskId(rows[0][0])
            self._execute(
                """
                INSERT INTO task_place (task_id, place)
                VALUES (?, (SELECT coalesce(max(place), 0) + 1 FROM task_place))
                """,
                (task_id,),
            )
        return task_id

    def swap_places(self, first: TaskId, second: TaskId) -> None:
        with self._transaction():
            for wanted in (first, second):
                self._require_known(wanted)
            places = {
                TaskId(task_id): place
                for task_id, place in self._execute(
                    "SELECT task_id, place FROM task_place WHERE task_id IN (?, ?)",
                    (first, second),
                )
            }
            for task_id, other in ((first, second), (second, first)):
                self._execute(
                    "UPDATE task_place SET place = ? WHERE task_id = ?",
                    (places[other], task_id),
                )

    def set_description(self, task_id: TaskId, description: Description) -> None:
        with self._transaction():
            self._require_pending(task_id)
            self._execute(
                "UPDATE task SET description = ? WHERE id = ?",
                (description.text, task_id),
            )

    def set_due(self, task_id: TaskId, due: datetime | None) -> None:
        with self._transaction():
            self._require_pending(task_id)
            self._execute("UPDATE task SET due = ? WHERE id = ?", (due, task_id))

    def set_project_due(self, project: ProjectPath, due: datetime | None) -> None:
        with self._transaction():
            self._execute(
                "DELETE FROM project_deadline WHERE project = ?",
                (str(project),),
            )
            if due is not None:
                self._execute(
                    "INSERT INTO project_deadline (project, due) VALUES (?, ?)",
                    (str(project), due),
                )

    def set_project(self, task_id: TaskId, project: ProjectPath | None) -> None:
        with self._transaction():
            self._require_pending(task_id)
            if project is not None:
                self._remember(project)
            self._execute(
                "UPDATE task SET project = ? WHERE id = ?",
                (None if project is None else str(project), task_id),
            )

    def add_project(self, project: ProjectPath) -> None:
        with self._transaction():
            self._remember(project)

    def forget_project(self, project: ProjectPath) -> None:
        beneath = "WHERE project = ? OR starts_with(project, ?)"
        arguments = (str(project), f"{project}{PROJECT_SEPARATOR}")
        with self._transaction():
            rows = self._execute(
                f"SELECT count(*) FROM task {beneath}",
                arguments,
            )
            held = int(rows[0][0])
            if held:
                raise ProjectNotEmpty(project, held)
            for table, column in (("project", "path"), ("project_deadline", "project")):
                self._execute(
                    f"DELETE FROM {table} "
                    f"WHERE {column} = ? OR starts_with({column}, ?)",
                    arguments,
                )

    def _remember(self, project: ProjectPath) -> None:
        """Record a project and everything enclosing it, each on the end.

        An enclosing project is recorded in its own right so that every row
        the tree shows is a project that can be arranged, renamed or dropped.
        """
        for step in (*project.ancestors(), project):
            self._execute(
                """
                INSERT INTO project (path, place)
                VALUES (?, (SELECT coalesce(max(place), 0) + 1 FROM project))
                ON CONFLICT DO NOTHING
                """,
                (str(step),),
            )

    def swap_project_places(self, first: ProjectPath, second: ProjectPath) -> None:
        with self._transaction():
            places = {
                ProjectPath.parse(path): place
                for path, place in self._execute(
                    "SELECT path, place FROM project WHERE path IN (?, ?)",
                    (str(first), str(second)),
                )
            }
            for wanted in (first, second):
                if wanted not in places:
                    raise UnknownProject(wanted)
            for project, other in ((first, second), (second, first)):
                self._execute(
                    "UPDATE project SET place = ? WHERE path = ?",
                    (places[other], str(project)),
                )

    def complete(self, task_id: TaskId, now: datetime) -> None:
        with self._transaction():
            self._require_pending(task_id)
            if self._is_running(task_id):
                self._close_interval(task_id, now)
            self._execute(
                """
                UPDATE task
                SET status = 'done', completed_at = ?
                WHERE id = ?
                """,
                (now, task_id),
            )

    def reopen(self, task_id: TaskId) -> None:
        with self._transaction():
            self._require_done(task_id)
            self._execute(
                """
                UPDATE task
                SET status = 'pending', completed_at = NULL
                WHERE id = ?
                """,
                (task_id,),
            )

    def start(self, task_id: TaskId, now: datetime) -> None:
        with self._transaction():
            self._require_pending(task_id)
            if self._is_running(task_id):
                return
            self._execute(
                "INSERT INTO running (task_id, started_at) VALUES (?, ?)",
                (task_id, now),
            )

    def stop(self, task_id: TaskId, now: datetime) -> None:
        with self._transaction():
            self._close_interval(task_id, now)

    def rename_project(self, old: ProjectPath, new: ProjectPath) -> int:
        # The match covers the project itself and everything beneath it, and
        # the substring keeps the segments below the old root, so renaming
        # `home` to `house` turns `home.garden` into `house.garden` rather
        # than flattening it.
        selection = "WHERE project = ? OR starts_with(project, ?)"
        arguments = (str(old), f"{old}{PROJECT_SEPARATOR}")
        with self._transaction():
            rows = self._execute(
                f"SELECT count(*) FROM task {selection}",
                arguments,
            )
            # A RETURNING clause here would trip DuckDB's foreign key check on
            # any task that has tracked time, so the count is taken up front.
            self._execute(
                f"UPDATE task SET project = ? || substr(project, ?) {selection}",
                (str(new), len(str(old)) + 1, *arguments),
            )
            # The project itself and its deadline travel with the rename.
            for table, column in (("project", "path"), ("project_deadline", "project")):
                self._execute(
                    f"UPDATE {table} SET {column} = ? || substr({column}, ?) "
                    f"WHERE {column} = ? OR starts_with({column}, ?)",
                    (str(new), len(str(old)) + 1, *arguments),
                )
            self._remember(new)
        return int(rows[0][0])

    def _migrate(self) -> None:
        self._connection.execute(
            "CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL)",
        )
        rows = self._connection.execute("SELECT max(version) FROM schema_version")
        current = rows.fetchall()[0][0]
        applied = 0 if current is None else int(current)
        for version, statements in enumerate(MIGRATIONS, start=1):
            if version <= applied:
                continue
            with self._transaction():
                for statement in statements:
                    self._connection.execute(statement)
                self._connection.execute(
                    "INSERT INTO schema_version (version) VALUES (?)",
                    (version,),
                )

    def _execute(self, sql: str, parameters: Sequence[Any] = ()) -> list[Any]:
        try:
            return self._connection.execute(sql, list(parameters)).fetchall()
        except duckdb.ConstraintException as error:
            raise StoreError(str(error).splitlines()[0]) from error

    def _require_pending(self, task_id: TaskId) -> None:
        rows = self._connection.execute(
            "SELECT status FROM task WHERE id = ?",
            [task_id],
        ).fetchall()
        if not rows:
            raise UnknownTask(task_id)
        if rows[0][0] != "pending":
            raise TaskAlreadyDone(task_id)

    def _require_known(self, task_id: TaskId) -> None:
        rows = self._connection.execute(
            "SELECT 1 FROM task WHERE id = ?",
            [task_id],
        ).fetchall()
        if not rows:
            raise UnknownTask(task_id)

    def _require_done(self, task_id: TaskId) -> None:
        rows = self._connection.execute(
            "SELECT status FROM task WHERE id = ?",
            [task_id],
        ).fetchall()
        if not rows:
            raise UnknownTask(task_id)
        if rows[0][0] != "done":
            raise TaskNotDone(task_id)

    def _is_running(self, task_id: TaskId) -> bool:
        rows = self._connection.execute(
            "SELECT 1 FROM running WHERE task_id = ?",
            [task_id],
        ).fetchall()
        return bool(rows)

    def _close_interval(self, task_id: TaskId, now: datetime) -> None:
        rows = self._connection.execute(
            "SELECT started_at FROM running WHERE task_id = ?",
            [task_id],
        ).fetchall()
        if not rows:
            raise NotRunning(task_id)
        started_at = rows[0][0]
        self._execute(
            """
            INSERT INTO work_interval (task_id, started_at, stopped_at)
            VALUES (?, ?, ?)
            """,
            (task_id, started_at, max(started_at, now)),
        )
        self._execute("DELETE FROM running WHERE task_id = ?", (task_id,))

    @contextmanager
    def _transaction(self) -> Iterator[None]:
        self._connection.begin()
        try:
            yield
        except BaseException:
            self._connection.rollback()
            raise
        else:
            self._connection.commit()


def _read_task(row: Sequence[Any]) -> Task:
    identifier, description, project, status, created_at, completed_at, due = row
    return Task(
        id=TaskId(identifier),
        description=Description(description),
        project=None if project is None else ProjectPath.parse(project),
        state=_read_state(status, completed_at),
        created_at=created_at,
        due=due,
    )


def _read_state(status: str, completed_at: datetime | None) -> TaskState:
    if status == "pending":
        return Pending()
    if completed_at is None:
        raise StoreError("a done task is missing its completion time")
    return Done(completed_at=completed_at)
