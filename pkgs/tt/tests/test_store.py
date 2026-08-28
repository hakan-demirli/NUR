"""Both stores must behave identically, so every test runs against both."""

from __future__ import annotations

from collections.abc import Iterator
from datetime import UTC, datetime, timedelta
from pathlib import Path

import duckdb
import pytest

from tasktui.task.duckdb_store import MIGRATIONS, DuckdbStore
from tasktui.task.model import (
    Description,
    Done,
    Pending,
    ProjectPath,
    TaskId,
    UnderProject,
    summary_scopes,
)
from tasktui.task.store import (
    InMemoryStore,
    NotRunning,
    ProjectNotEmpty,
    TaskAlreadyDone,
    TaskNotDone,
    TaskStore,
    UnknownProject,
    UnknownTask,
)

EPOCH = datetime(2026, 1, 1, tzinfo=UTC)
LATER = EPOCH + timedelta(hours=1)


@pytest.fixture(params=["memory", "duckdb"])
def store(request: pytest.FixtureRequest) -> Iterator[TaskStore]:
    made: TaskStore = (
        InMemoryStore() if request.param == "memory" else DuckdbStore.in_memory()
    )
    yield made
    made.close()


def described(text: str) -> Description:
    return Description.parse(text)


def path(text: str) -> ProjectPath:
    return ProjectPath.parse(text)


def test_an_added_task_comes_back_pending(store: TaskStore) -> None:
    identifier = store.add(described("read the docs"), path("home"), EPOCH)
    task = store.snapshot().find(identifier)
    assert task is not None
    assert task.description == described("read the docs")
    assert task.project == path("home")
    assert isinstance(task.state, Pending)
    assert store.snapshot().elapsed(task, LATER) == timedelta()


def test_a_task_may_have_no_project(store: TaskStore) -> None:
    identifier = store.add(described("loose"), None, EPOCH)
    task = store.snapshot().find(identifier)
    assert task is not None
    assert task.project is None


def test_identifiers_are_not_reused(store: TaskStore) -> None:
    first = store.add(described("one"), None, EPOCH)
    second = store.add(described("two"), None, EPOCH)
    store.complete(first, LATER)
    third = store.add(described("three"), None, EPOCH)
    assert len({first, second, third}) == 3


def test_completing_records_when(store: TaskStore) -> None:
    identifier = store.add(described("one"), None, EPOCH)
    store.complete(identifier, LATER)
    task = store.snapshot().find(identifier)
    assert task is not None
    assert task.state == Done(completed_at=LATER)


def test_completing_twice_is_refused(store: TaskStore) -> None:
    identifier = store.add(described("one"), None, EPOCH)
    store.complete(identifier, LATER)
    with pytest.raises(TaskAlreadyDone):
        store.complete(identifier, LATER)


def test_acting_on_a_missing_task_is_refused(store: TaskStore) -> None:
    missing = TaskId(4321)
    with pytest.raises(UnknownTask):
        store.complete(missing, EPOCH)
    with pytest.raises(UnknownTask):
        store.start(missing, EPOCH)
    with pytest.raises(UnknownTask):
        store.set_description(missing, described("nope"))


def test_a_description_can_be_replaced(store: TaskStore) -> None:
    identifier = store.add(described("typo"), None, EPOCH)
    store.set_description(identifier, described("fixed"))
    task = store.snapshot().find(identifier)
    assert task is not None
    assert task.description == described("fixed")


def test_a_task_can_be_moved_into_a_new_project(store: TaskStore) -> None:
    identifier = store.add(described("loose"), None, EPOCH)
    store.set_project(identifier, path("work.email"))
    task = store.snapshot().find(identifier)
    assert task is not None
    assert str(task.project) == "work.email"


def test_a_task_can_be_taken_out_of_its_project(store: TaskStore) -> None:
    identifier = store.add(described("filed"), path("work"), EPOCH)
    store.set_project(identifier, None)
    task = store.snapshot().find(identifier)
    assert task is not None
    assert task.project is None


def test_moving_a_missing_task_is_refused(store: TaskStore) -> None:
    with pytest.raises(UnknownTask):
        store.set_project(TaskId(4321), path("work"))


def test_moving_a_task_with_tracked_time_works(store: TaskStore) -> None:
    identifier = store.add(described("tracked"), None, EPOCH)
    store.start(identifier, EPOCH)
    store.stop(identifier, EPOCH + timedelta(minutes=3))
    store.set_project(identifier, path("work"))
    snapshot = store.snapshot()
    task = snapshot.find(identifier)
    assert task is not None
    assert str(task.project) == "work"
    assert snapshot.elapsed(task, LATER) == timedelta(minutes=3)


def test_the_clock_accumulates_across_runs(store: TaskStore) -> None:
    identifier = store.add(described("one"), None, EPOCH)
    store.start(identifier, EPOCH)
    store.stop(identifier, EPOCH + timedelta(minutes=30))
    store.start(identifier, EPOCH + timedelta(hours=2))
    store.stop(identifier, EPOCH + timedelta(hours=2, minutes=15))
    snapshot = store.snapshot()
    task = snapshot.find(identifier)
    assert task is not None
    assert snapshot.elapsed(task, LATER) == timedelta(minutes=45)


def test_several_clocks_run_at_once(store: TaskStore) -> None:
    """Starting one task leaves any other running."""
    first = store.add(described("one"), None, EPOCH)
    second = store.add(described("two"), None, EPOCH)
    store.start(first, EPOCH)
    store.start(second, EPOCH + timedelta(minutes=10))
    snapshot = store.snapshot()
    assert {clock.task_id for clock in snapshot.running} == {first, second}
    assert snapshot.is_running(first)
    assert snapshot.is_running(second)


def test_overlapping_clocks_are_counted_once_in_a_total(store: TaskStore) -> None:
    first = store.add(described("one"), None, EPOCH)
    second = store.add(described("two"), None, EPOCH)
    store.start(first, EPOCH)
    store.start(second, EPOCH)
    store.stop(first, EPOCH + timedelta(hours=1))
    store.stop(second, EPOCH + timedelta(hours=1))
    snapshot = store.snapshot()
    assert snapshot.elapsed(snapshot.tasks[0], LATER) == timedelta(hours=1)
    assert snapshot.elapsed(snapshot.tasks[1], LATER) == timedelta(hours=1)
    assert snapshot.spent_on(snapshot.tasks, LATER) == timedelta(hours=1)


def test_starting_a_running_task_again_does_not_restart_it(store: TaskStore) -> None:
    identifier = store.add(described("one"), None, EPOCH)
    store.start(identifier, EPOCH)
    store.start(identifier, EPOCH + timedelta(minutes=30))
    snapshot = store.snapshot()
    assert len(snapshot.running) == 1
    assert snapshot.running[0].since == EPOCH


def test_stopping_one_clock_leaves_the_others_running(store: TaskStore) -> None:
    first = store.add(described("one"), None, EPOCH)
    second = store.add(described("two"), None, EPOCH)
    store.start(first, EPOCH)
    store.start(second, EPOCH)
    store.stop(first, EPOCH + timedelta(minutes=5))
    snapshot = store.snapshot()
    assert [clock.task_id for clock in snapshot.running] == [second]


def test_completing_a_running_task_banks_its_time(store: TaskStore) -> None:
    identifier = store.add(described("one"), None, EPOCH)
    store.start(identifier, EPOCH)
    store.complete(identifier, EPOCH + timedelta(minutes=20))
    snapshot = store.snapshot()
    assert snapshot.running == ()
    task = snapshot.find(identifier)
    assert task is not None
    assert snapshot.elapsed(task, LATER) == timedelta(minutes=20)


def test_stopping_an_idle_clock_is_refused(store: TaskStore) -> None:
    identifier = store.add(described("one"), None, EPOCH)
    with pytest.raises(NotRunning):
        store.stop(identifier, EPOCH)


def test_starting_a_completed_task_is_refused(store: TaskStore) -> None:
    identifier = store.add(described("one"), None, EPOCH)
    store.complete(identifier, LATER)
    with pytest.raises(TaskAlreadyDone):
        store.start(identifier, LATER)


def listed(store: TaskStore) -> list[str]:
    return [task.description.text for task in store.snapshot().tasks]


def test_a_new_task_goes_on_the_end_of_the_list(store: TaskStore) -> None:
    for text in ("first", "second", "third"):
        store.add(described(text), None, EPOCH)
    assert listed(store) == ["first", "second", "third"]


def test_two_tasks_can_change_places(store: TaskStore) -> None:
    first = store.add(described("first"), None, EPOCH)
    second = store.add(described("second"), None, EPOCH)
    store.swap_places(first, second)
    assert listed(store) == ["second", "first"]


def test_changing_places_leaves_the_others_alone(store: TaskStore) -> None:
    identifiers = [
        store.add(described(text), None, EPOCH) for text in ("a", "b", "c", "d")
    ]
    store.swap_places(identifiers[0], identifiers[2])
    assert listed(store) == ["c", "b", "a", "d"]


def test_an_order_arranged_by_hand_outlives_a_new_task(store: TaskStore) -> None:
    first = store.add(described("first"), None, EPOCH)
    second = store.add(described("second"), None, EPOCH)
    store.swap_places(first, second)
    store.add(described("third"), None, EPOCH)
    assert listed(store) == ["second", "first", "third"]


def test_an_order_arranged_by_hand_outlives_finishing_something(
    store: TaskStore,
) -> None:
    first = store.add(described("first"), None, EPOCH)
    store.add(described("second"), None, EPOCH)
    store.complete(first, LATER)
    assert listed(store) == ["first", "second"]


def test_places_cannot_be_swapped_with_something_unknown(store: TaskStore) -> None:
    known = store.add(described("one"), None, EPOCH)
    with pytest.raises(UnknownTask):
        store.swap_places(known, TaskId(404))
    with pytest.raises(UnknownTask):
        store.swap_places(TaskId(404), known)


def test_a_finished_task_can_be_put_back(store: TaskStore) -> None:
    identifier = store.add(described("one"), None, EPOCH)
    store.complete(identifier, LATER)
    store.reopen(identifier)
    task = store.snapshot().find(identifier)
    assert task is not None
    assert isinstance(task.state, Pending)


def test_reopening_leaves_the_tracked_time_alone(store: TaskStore) -> None:
    identifier = store.add(described("one"), None, EPOCH)
    store.start(identifier, EPOCH)
    store.complete(identifier, LATER)

    def tracked() -> timedelta:
        snapshot = store.snapshot()
        task = snapshot.find(identifier)
        assert task is not None
        return snapshot.elapsed(task, LATER)

    before = tracked()
    store.reopen(identifier)
    assert tracked() == before


def test_a_task_still_to_do_cannot_be_reopened(store: TaskStore) -> None:
    identifier = store.add(described("one"), None, EPOCH)
    with pytest.raises(TaskNotDone):
        store.reopen(identifier)


def test_reopening_something_unknown_is_refused(store: TaskStore) -> None:
    with pytest.raises(UnknownTask):
        store.reopen(TaskId(404))


def test_a_reopened_task_can_be_finished_again(store: TaskStore) -> None:
    identifier = store.add(described("one"), None, EPOCH)
    store.complete(identifier, LATER)
    store.reopen(identifier)
    store.complete(identifier, LATER)
    task = store.snapshot().find(identifier)
    assert task is not None
    assert isinstance(task.state, Done)


def test_a_project_can_be_made_before_it_holds_anything(store: TaskStore) -> None:
    store.add_project(path("greenhouse"))
    assert path("greenhouse") in store.snapshot().projects


def test_putting_a_task_in_a_project_records_the_project(store: TaskStore) -> None:
    store.add(described("one"), path("home.garden"), EPOCH)
    assert path("home.garden") in store.snapshot().projects


def test_moving_a_task_records_the_project_it_moved_to(store: TaskStore) -> None:
    identifier = store.add(described("one"), None, EPOCH)
    store.set_project(identifier, path("work.ops"))
    assert path("work.ops") in store.snapshot().projects


def test_a_project_outlives_the_last_task_in_it(store: TaskStore) -> None:
    """Finishing the work is not the same as abandoning the project."""
    identifier = store.add(described("one"), path("home"), EPOCH)
    store.complete(identifier, LATER)
    assert path("home") in store.snapshot().projects


def test_making_the_same_project_twice_is_harmless(store: TaskStore) -> None:
    store.add_project(path("home"))
    store.add_project(path("home"))
    assert path("home") in store.snapshot().projects


def tree(store: TaskStore) -> list[str]:
    return [
        str(scope.path)
        for scope in summary_scopes(store.snapshot())
        if isinstance(scope, UnderProject)
    ]


def test_a_new_project_goes_on_the_end_of_the_tree(store: TaskStore) -> None:
    for name in ("work", "home", "admin"):
        store.add_project(path(name))
    assert tree(store) == ["work", "home", "admin"]


def test_enclosing_projects_are_recorded_in_their_own_right(store: TaskStore) -> None:
    """Every row of the tree has to be something that can be arranged."""
    store.add_project(path("home.garden.beds"))
    assert tree(store) == ["home", "home.garden", "home.garden.beds"]


def test_two_projects_can_change_places(store: TaskStore) -> None:
    store.add_project(path("work"))
    store.add_project(path("home"))
    store.swap_project_places(path("work"), path("home"))
    assert tree(store) == ["home", "work"]


def test_moving_a_project_takes_its_branch_along(store: TaskStore) -> None:
    store.add_project(path("work.ops"))
    store.add_project(path("home.garden"))
    store.swap_project_places(path("work"), path("home"))
    assert tree(store) == ["home", "home.garden", "work", "work.ops"]


def test_projects_inside_one_project_can_be_arranged(store: TaskStore) -> None:
    for name in ("home.garden", "home.attic"):
        store.add_project(path(name))
    store.swap_project_places(path("home.garden"), path("home.attic"))
    assert tree(store) == ["home", "home.attic", "home.garden"]


def test_an_arranged_tree_outlives_a_new_project(store: TaskStore) -> None:
    store.add_project(path("work"))
    store.add_project(path("home"))
    store.swap_project_places(path("work"), path("home"))
    store.add_project(path("admin"))
    assert tree(store) == ["home", "work", "admin"]


def test_places_cannot_be_swapped_with_an_unknown_project(store: TaskStore) -> None:
    store.add_project(path("home"))
    with pytest.raises(UnknownProject):
        store.swap_project_places(path("home"), path("nowhere"))
    with pytest.raises(UnknownProject):
        store.swap_project_places(path("nowhere"), path("home"))


def test_an_empty_project_can_be_forgotten(store: TaskStore) -> None:
    store.add_project(path("mistake"))
    store.forget_project(path("mistake"))
    assert path("mistake") not in store.snapshot().projects


def test_a_project_holding_work_is_not_forgotten(store: TaskStore) -> None:
    store.add(described("one"), path("home"), EPOCH)
    with pytest.raises(ProjectNotEmpty):
        store.forget_project(path("home"))
    assert path("home") in store.snapshot().projects


def test_a_project_holding_finished_work_is_not_forgotten(store: TaskStore) -> None:
    """The task is done but its history still points at the project."""
    identifier = store.add(described("one"), path("home"), EPOCH)
    store.complete(identifier, LATER)
    with pytest.raises(ProjectNotEmpty):
        store.forget_project(path("home"))


def test_forgetting_a_project_takes_its_children_with_it(store: TaskStore) -> None:
    store.add_project(path("home"))
    store.add_project(path("home.garden"))
    store.forget_project(path("home"))
    assert store.snapshot().projects == {}


def test_forgetting_a_project_drops_its_deadline(store: TaskStore) -> None:
    store.add_project(path("home"))
    store.set_project_due(path("home"), LATER)
    store.forget_project(path("home"))
    assert store.snapshot().project_due == {}


def test_renaming_carries_the_project_record_along(store: TaskStore) -> None:
    store.add_project(path("home.garden"))
    store.rename_project(path("home"), path("house"))
    projects = store.snapshot().projects
    assert path("house.garden") in projects
    assert path("home.garden") not in projects


def test_renaming_moves_the_project_and_its_children(store: TaskStore) -> None:
    store.add(described("at the root"), path("home"), EPOCH)
    store.add(described("one down"), path("home.garden"), EPOCH)
    store.add(described("two down"), path("home.garden.shed"), EPOCH)
    moved = store.rename_project(path("home"), path("house"))
    assert moved == 3
    projects = {
        task.description.text: str(task.project) for task in store.snapshot().tasks
    }
    assert projects == {
        "at the root": "house",
        "one down": "house.garden",
        "two down": "house.garden.shed",
    }


def test_renaming_leaves_similarly_named_projects_alone(store: TaskStore) -> None:
    """`homework` merely starts with `home`; it is not beneath it."""
    store.add(described("inside"), path("home"), EPOCH)
    store.add(described("outside"), path("homework"), EPOCH)
    store.add(described("unrelated"), path("work"), EPOCH)
    store.add(described("none at all"), None, EPOCH)
    assert store.rename_project(path("home"), path("house")) == 1
    projects = {
        task.description.text: None if task.project is None else str(task.project)
        for task in store.snapshot().tasks
    }
    assert projects == {
        "inside": "house",
        "outside": "homework",
        "unrelated": "work",
        "none at all": None,
    }


def test_renaming_a_project_with_tracked_time_works(store: TaskStore) -> None:
    """DuckDB refuses some updates to rows referenced by a foreign key."""
    identifier = store.add(described("tracked"), path("home"), EPOCH)
    store.start(identifier, EPOCH)
    store.stop(identifier, EPOCH + timedelta(minutes=5))
    assert store.rename_project(path("home"), path("house")) == 1
    snapshot = store.snapshot()
    task = snapshot.find(identifier)
    assert task is not None
    assert str(task.project) == "house"
    assert snapshot.elapsed(task, LATER) == timedelta(minutes=5)


def test_renaming_carries_the_deadline_along(store: TaskStore) -> None:
    store.set_project_due(path("home"), LATER)
    store.rename_project(path("home"), path("house"))
    assert store.snapshot().project_due == {path("house"): LATER}


def test_renaming_can_move_a_project_deeper(store: TaskStore) -> None:
    store.add(described("one"), path("garden"), EPOCH)
    store.rename_project(path("garden"), path("home.garden"))
    task = store.snapshot().tasks[0]
    assert str(task.project) == "home.garden"


def test_renaming_a_project_that_holds_nothing_changes_nothing(
    store: TaskStore,
) -> None:
    assert store.rename_project(path("ghost"), path("phantom")) == 0


def test_timestamps_survive_a_round_trip_in_utc(store: TaskStore) -> None:
    identifier = store.add(described("one"), None, EPOCH)
    task = store.snapshot().find(identifier)
    assert task is not None
    assert task.created_at == EPOCH


def at_version(database: Path, version: int) -> None:
    """Build a database with only the first ``version`` migrations applied.

    An upgrade has to be tried against a database with something in it.  A
    fresh one is empty when a migration runs, which is exactly the case that
    says least about whether the migration works.
    """
    connection = duckdb.connect(str(database))
    connection.execute(
        "CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL)",
    )
    for applied, statements in enumerate(MIGRATIONS[:version], start=1):
        for statement in statements:
            connection.execute(statement)
        connection.execute("INSERT INTO schema_version (version) VALUES (?)", [applied])
    connection.close()


@pytest.mark.parametrize("start", range(len(MIGRATIONS)))
def test_a_database_upgrades_from_any_earlier_version(
    tmp_path: Path,
    start: int,
) -> None:
    database = tmp_path / f"from-{start}.duckdb"
    at_version(database, start)
    store = DuckdbStore.open(database)
    assert store.snapshot().tasks == ()
    store.close()


def test_a_tree_with_projects_in_it_survives_being_upgraded(tmp_path: Path) -> None:
    """Version 5 is the last before the tree had an order of its own."""
    database = tmp_path / "tasks.duckdb"
    at_version(database, 5)
    connection = duckdb.connect(str(database))
    for name in ("work.ops", "home", "errands"):
        connection.execute("INSERT INTO project (path) VALUES (?)", [name])
    connection.close()

    store = DuckdbStore.open(database)
    assert tree(store) == ["errands", "home", "work", "work.ops"]
    store.close()


def test_a_list_with_tasks_in_it_survives_being_upgraded(tmp_path: Path) -> None:
    """Version 4 is the last before the list had an order of its own."""
    database = tmp_path / "tasks.duckdb"
    at_version(database, 4)
    connection = duckdb.connect(str(database))
    for text in ("first", "second"):
        connection.execute(
            "INSERT INTO task (description, project, status, created_at) "
            "VALUES (?, NULL, 'pending', ?)",
            [text, EPOCH],
        )
    connection.close()

    store = DuckdbStore.open(database)
    assert listed(store) == ["first", "second"]
    store.close()


def test_an_order_arranged_by_hand_survives_a_restart(tmp_path: Path) -> None:
    database = tmp_path / "tasks.duckdb"
    first = DuckdbStore.open(database)
    one = first.add(described("first"), None, EPOCH)
    two = first.add(described("second"), None, EPOCH)
    first.swap_places(one, two)
    first.close()

    second = DuckdbStore.open(database)
    assert listed(second) == ["second", "first"]
    second.close()


def test_duckdb_persists_across_connections(tmp_path: Path) -> None:
    database = tmp_path / "nested" / "tasks.duckdb"
    first = DuckdbStore.open(database)
    identifier = first.add(described("remember me"), path("home"), EPOCH)
    first.start(identifier, EPOCH)
    first.stop(identifier, EPOCH + timedelta(minutes=7))
    first.close()

    second = DuckdbStore.open(database)
    snapshot = second.snapshot()
    task = snapshot.find(identifier)
    assert task is not None
    assert task.description == described("remember me")
    assert snapshot.elapsed(task, LATER) == timedelta(minutes=7)
    second.close()


def test_running_clocks_survive_a_restart(tmp_path: Path) -> None:
    database = tmp_path / "tasks.duckdb"
    first = DuckdbStore.open(database)
    one = first.add(described("one"), None, EPOCH)
    two = first.add(described("two"), None, EPOCH)
    first.start(one, EPOCH)
    first.start(two, EPOCH)
    first.close()

    second = DuckdbStore.open(database)
    snapshot = second.snapshot()
    assert {clock.task_id for clock in snapshot.running} == {one, two}
    second.close()


def test_reopening_does_not_reapply_migrations(tmp_path: Path) -> None:
    database = tmp_path / "tasks.duckdb"
    for _ in range(3):
        store = DuckdbStore.open(database)
        store.close()
    store = DuckdbStore.open(database)
    assert store.snapshot().tasks == ()
    store.close()
