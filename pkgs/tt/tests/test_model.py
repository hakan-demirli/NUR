from __future__ import annotations

from datetime import UTC, datetime, timedelta

import pytest

from tasktui.task.model import (
    AllProjects,
    Description,
    Done,
    Interval,
    InvalidDescription,
    InvalidProjectPath,
    NoProject,
    Pending,
    ProjectPath,
    Snapshot,
    Task,
    TaskId,
    UnderProject,
    merged_duration,
    summarize,
)

EPOCH = datetime(2026, 1, 1, tzinfo=UTC)


def task(
    identifier: int,
    description: str = "something",
    project: str | None = None,
    done: bool = False,
) -> Task:
    return Task(
        id=TaskId(identifier),
        description=Description(description),
        project=None if project is None else ProjectPath.parse(project),
        state=Done(completed_at=EPOCH) if done else Pending(),
        created_at=EPOCH,
    )


@pytest.mark.parametrize(
    ("text", "segments"),
    [
        ("home", ("home",)),
        ("home.garden", ("home", "garden")),
        ("  home.garden  ", ("home", "garden")),
        ("a.b.c.d", ("a", "b", "c", "d")),
    ],
)
def test_project_path_parses(text: str, segments: tuple[str, ...]) -> None:
    assert ProjectPath.parse(text).segments == segments


@pytest.mark.parametrize("text", ["", "   ", ".", "home.", ".home", "a..b", "a. .b"])
def test_project_path_rejects_malformed(text: str) -> None:
    with pytest.raises(InvalidProjectPath):
        ProjectPath.parse(text)


@pytest.mark.parametrize(
    ("text", "segments"),
    [
        ("huge pp", ("huge pp",)),
        ("Home Renovation.Kitchen", ("Home Renovation", "Kitchen")),
        ("home . garden", ("home", "garden")),
        ("  spaced out  ", ("spaced out",)),
    ],
)
def test_project_names_may_contain_spaces(
    text: str,
    segments: tuple[str, ...],
) -> None:
    """Nothing needs them forbidden: paths split on a dot, not on a space."""
    assert ProjectPath.parse(text).segments == segments


@pytest.mark.parametrize("text", ["home\tgarden", "home\ngarden", "a.b\x07c"])
def test_project_names_reject_characters_that_cannot_be_drawn(text: str) -> None:
    with pytest.raises(InvalidProjectPath):
        ProjectPath.parse(text)


def test_descriptions_may_contain_spaces_but_not_newlines() -> None:
    assert Description.parse("buy some compost").text == "buy some compost"
    with pytest.raises(InvalidDescription):
        Description.parse("two\nlines")


def test_project_path_round_trips_through_text() -> None:
    assert str(ProjectPath.parse("home.garden.shed")) == "home.garden.shed"


def test_project_path_reports_its_ancestry() -> None:
    path = ProjectPath.parse("a.b.c")
    assert path.name == "c"
    assert path.depth == 2
    assert path.parent == ProjectPath.parse("a.b")
    assert path.ancestors() == (ProjectPath.parse("a"), ProjectPath.parse("a.b"))


def test_top_level_project_has_no_parent() -> None:
    path = ProjectPath.parse("a")
    assert path.parent is None
    assert path.ancestors() == ()


def test_project_path_contains_itself_and_descendants() -> None:
    home = ProjectPath.parse("home")
    assert home.contains(home)
    assert home.contains(ProjectPath.parse("home.garden"))
    assert not home.contains(ProjectPath.parse("homework"))
    assert not ProjectPath.parse("home.garden").contains(home)


def test_rebasing_preserves_the_segments_below_the_root() -> None:
    old = ProjectPath.parse("home")
    new = ProjectPath.parse("house")
    assert ProjectPath.parse("home").rebased(old, new) == new
    assert ProjectPath.parse("home.garden.shed").rebased(old, new) == ProjectPath.parse(
        "house.garden.shed"
    )


def test_rebasing_leaves_unrelated_paths_alone() -> None:
    old = ProjectPath.parse("home")
    new = ProjectPath.parse("house")
    unrelated = ProjectPath.parse("work.email")
    assert unrelated.rebased(old, new) == unrelated


def test_description_rejects_blank_text() -> None:
    with pytest.raises(InvalidDescription):
        Description.parse("   ")


def test_description_trims_surrounding_space() -> None:
    assert Description.parse("  buy milk  ").text == "buy milk"


def test_task_matches_the_relevant_filters() -> None:
    gardening = task(1, project="home.garden")
    loose = task(2)

    assert gardening.matches(AllProjects())
    assert gardening.matches(UnderProject(ProjectPath.parse("home")))
    assert not gardening.matches(NoProject())

    assert loose.matches(AllProjects())
    assert loose.matches(NoProject())
    assert not loose.matches(UnderProject(ProjectPath.parse("home")))


def test_summarize_always_leads_with_every_task() -> None:
    empty = Snapshot(tasks=())
    populated = Snapshot(tasks=(task(1, project="home"), task(2)))
    assert summarize(empty, EPOCH)[0].scope == AllProjects()
    assert summarize(populated, EPOCH)[0].scope == AllProjects()
    assert summarize(populated, EPOCH)[0].pending == 2


def test_summarize_synthesises_missing_ancestors() -> None:
    snapshot = Snapshot(tasks=(task(1, project="a.b.c"),))
    scopes = [summary.scope for summary in summarize(snapshot, EPOCH)]
    assert scopes == [
        AllProjects(),
        UnderProject(ProjectPath.parse("a")),
        UnderProject(ProjectPath.parse("a.b")),
        UnderProject(ProjectPath.parse("a.b.c")),
    ]


def test_summarize_rolls_counts_up_the_tree() -> None:
    snapshot = Snapshot(
        tasks=(
            task(1, project="home"),
            task(2, project="home.garden"),
            task(3, project="home.garden"),
            task(4, project="work"),
        )
    )
    counts = {
        str(summary.scope.path): summary.pending
        for summary in summarize(snapshot, EPOCH)
        if isinstance(summary.scope, UnderProject)
    }
    assert counts == {"home": 3, "home.garden": 2, "work": 1}


def test_summarize_puts_unassigned_tasks_above_the_projects() -> None:
    snapshot = Snapshot(tasks=(task(1, project="home"), task(2)))
    scopes = [summary.scope for summary in summarize(snapshot, EPOCH)]
    assert scopes.index(NoProject()) < scopes.index(
        UnderProject(ProjectPath.parse("home"))
    )


def test_summarize_omits_the_unassigned_row_when_empty() -> None:
    snapshot = Snapshot(tasks=(task(1, project="home"),))
    assert all(summary.scope != NoProject() for summary in summarize(snapshot, EPOCH))


def test_summarize_counts_only_what_is_left_to_do() -> None:
    snapshot = Snapshot(
        tasks=(task(1, project="home"), task(2, project="home", done=True)),
    )
    counted = {summary.scope: summary.pending for summary in summarize(snapshot, EPOCH)}
    assert counted[UnderProject(ProjectPath.parse("home"))] == 1


def test_a_project_stays_on_the_tree_once_its_work_is_done() -> None:
    """The finished task is still listed under it, so the row has to remain."""
    snapshot = Snapshot(tasks=(task(1, project="home", done=True),))
    assert [summary.scope for summary in summarize(snapshot, EPOCH)] == [
        AllProjects(),
        UnderProject(ProjectPath.parse("home")),
    ]


def test_the_unassigned_row_stays_once_its_work_is_done() -> None:
    snapshot = Snapshot(tasks=(task(1, done=True),))
    assert NoProject() in [summary.scope for summary in summarize(snapshot, EPOCH)]


def spans(*pieces: tuple[int, int, int | None]) -> tuple[Interval, ...]:
    """Intervals written as (task, start minute, end minute or None)."""
    return tuple(
        Interval(
            task_id=TaskId(who),
            started_at=EPOCH + timedelta(minutes=begin),
            stopped_at=None if end is None else EPOCH + timedelta(minutes=end),
        )
        for who, begin, end in pieces
    )


def test_elapsed_includes_the_run_in_progress() -> None:
    running = task(1)
    snapshot = Snapshot(tasks=(running,), intervals=spans((1, 0, 5), (1, 10, None)))
    assert snapshot.elapsed(running, EPOCH + timedelta(minutes=12)) == timedelta(
        minutes=7
    )


def test_elapsed_ignores_other_tasks_runs() -> None:
    idle = task(1)
    snapshot = Snapshot(tasks=(idle,), intervals=spans((1, 0, 5), (99, 0, 60)))
    assert snapshot.elapsed(idle, EPOCH + timedelta(minutes=12)) == timedelta(minutes=5)


def test_overlapping_clocks_count_once_towards_a_total() -> None:
    """An hour with two clocks running is still only an hour."""
    both = (task(1, project="home"), task(2, project="home"))
    snapshot = Snapshot(tasks=both, intervals=spans((1, 0, 60), (2, 0, 60)))
    now = EPOCH + timedelta(hours=2)
    assert snapshot.elapsed(both[0], now) == timedelta(hours=1)
    assert snapshot.elapsed(both[1], now) == timedelta(hours=1)
    assert snapshot.spent_on(both, now) == timedelta(hours=1)


def test_partly_overlapping_clocks_merge_into_one_stretch() -> None:
    both = (task(1), task(2))
    snapshot = Snapshot(tasks=both, intervals=spans((1, 0, 30), (2, 20, 50)))
    assert snapshot.spent_on(both, EPOCH + timedelta(hours=2)) == timedelta(minutes=50)


def test_separate_clocks_add_up() -> None:
    both = (task(1), task(2))
    snapshot = Snapshot(tasks=both, intervals=spans((1, 0, 10), (2, 30, 45)))
    assert snapshot.spent_on(both, EPOCH + timedelta(hours=2)) == timedelta(minutes=25)


def test_a_running_clock_counts_up_to_now() -> None:
    one = (task(1),)
    snapshot = Snapshot(tasks=one, intervals=spans((1, 0, None)))
    assert snapshot.spent_on(one, EPOCH + timedelta(minutes=8)) == timedelta(minutes=8)


def test_nothing_tracked_is_no_time() -> None:
    assert merged_duration((), EPOCH) == timedelta()
