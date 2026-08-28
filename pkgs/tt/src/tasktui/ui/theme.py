"""How the interface looks, in one place.

There are three layers, each written in terms of the one before it.

The palette names colours by the job they do rather than by what they are, so
that the whole interface can be recoloured from one line.  The styles say how
each part of the screen is drawn, in terms of the palette.  The glyphs are the
few characters the interface draws that are not content.

Colours default to the terminal's own sixteen, so the interface takes on
whatever scheme is already configured rather than fighting it.  A palette
entry may equally be given as a hex value if a fixed appearance is wanted.
"""

from __future__ import annotations

from dataclasses import dataclass, fields, replace
from typing import Final, Self

from rich.color import ColorParseError
from rich.errors import StyleSyntaxError
from rich.style import Style
from rich.theme import Theme

CURSOR: Final = "tasktui.cursor"
TITLE: Final = "tasktui.title"
MUTED: Final = "tasktui.muted"
HEADER: Final = "tasktui.header"
NOTE: Final = "tasktui.note"
PROBLEM: Final = "tasktui.problem"
ACTIVE: Final = "tasktui.active"
CARET: Final = "tasktui.caret"
DUE: Final = "tasktui.due"
OVERDUE: Final = "tasktui.overdue"
DONE: Final = "tasktui.done"
GROUND_AGENDA: Final = "tasktui.ground_agenda"
GROUND_PROJECTS: Final = "tasktui.ground_projects"
GROUND_TASKS: Final = "tasktui.ground_tasks"
BAR: Final = "tasktui.bar"
TAB: Final = "tasktui.tab"
TAB_CHOSEN: Final = "tasktui.tab_chosen"
TRAIL: Final = "tasktui.trail"
COUNT: Final = "tasktui.count"


class UnknownStyle(ValueError):
    """Raised when a style, colour or glyph cannot be understood."""


@dataclass(frozen=True, slots=True)
class Palette:
    """The colours, under the Material 3 role names.

    The values are the dark scheme that Dracula and Material 3 agree on: the
    surfaces are tonal steps either side of the terminal's own background, and
    the accents are Dracula's published hues.

    A terminal has no notion of transparency.  There is no alpha channel in
    the escape codes, and a terminal's own opacity setting applies to the
    whole window rather than to a cell, so a highlight is an opaque colour or
    it is nothing.  Depth comes from tone instead: ``bar`` and ``selection``
    sit one and two steps above the background, which is what makes them read
    as raised rather than as a block of colour.

    Any entry may be replaced with a terminal colour name such as ``"cyan"``
    if the interface should follow the terminal's scheme instead.
    """

    # Surfaces, in tonal order from the terminal's own background upwards.
    bar: str = "#323440"  # surfaceContainerHighest
    selection: str = "#44475a"  # outlineVariant, Dracula's selection

    # One ground per pane, so that whichever pane is in front is recognisable
    # before it has been read.  They are a hair apart, and all a hair off the
    # Dracula background, which is enough to tell them apart without any of
    # them reading as a block of colour.  Set all three the same to have the
    # panes look alike again.
    ground_agenda: str = "#2a2b38"  # surface, a step towards the accent
    ground_projects: str = "#262b33"  # surface, a step towards the cyan
    ground_tasks: str = "#2b2a31"  # surface, a step towards the pink

    # Content.
    on_surface: str = "#f8f8f2"  # onSurface
    on_surface_variant: str = "#c5c5d5"  # onSurfaceVariant
    outline: str = "#6272a4"  # outline

    # Accents.
    accent: str = "#bd93f9"  # primary
    accent_container: str = "#593090"  # primaryContainer
    on_accent_container: str = "#eddcff"  # onPrimaryContainer
    alarm: str = "#ff5555"  # error
    success: str = "#50fa7b"  # success
    running: str = "#ffb86c"  # warning


@dataclass(frozen=True, slots=True)
class Glyphs:
    """The characters the interface draws that are not content."""

    active: str = "*"
    done: str = "x"
    caret: str = "_"
    indent: str = "  "
    gap: str = "  "
    breadcrumb: str = "\N{SINGLE RIGHT-POINTING ANGLE QUOTATION MARK}"


DEFAULT_PALETTE: Final = Palette()
DEFAULT_GLYPHS: Final = Glyphs()


def styles_for(palette: Palette) -> dict[str, str]:
    """Every style, written in terms of the palette rather than of colours."""
    return {
        # The cursor sets only a background, so a row keeps the colours of its
        # own cells rather than being flattened to one.
        CURSOR: f"on {palette.selection}",
        TITLE: f"bold {palette.accent}",
        MUTED: palette.outline,
        # What each column holds is chrome, not content, but it still has to
        # be read.  Bold and a tone below the rows keeps it apart from them
        # without dimming it to the point of looking like finished work.
        HEADER: f"bold {palette.on_surface_variant}",
        NOTE: palette.success,
        PROBLEM: f"bold {palette.alarm}",
        ACTIVE: f"bold {palette.running}",
        CARET: "blink",
        DUE: palette.accent,
        OVERDUE: f"bold {palette.alarm}",
        # Finished work stays on the pane as a record of itself, faded back so
        # it never competes with what is still to be done.
        DONE: palette.outline,
        GROUND_AGENDA: f"on {palette.ground_agenda}",
        GROUND_PROJECTS: f"on {palette.ground_projects}",
        GROUND_TASKS: f"on {palette.ground_tasks}",
        # The bar runs the full width, so its own colours carry the padding
        # either side of the labels.
        BAR: f"{palette.on_surface_variant} on {palette.bar}",
        TAB: f"{palette.on_surface_variant} on {palette.bar}",
        TAB_CHOSEN: f"bold {palette.on_accent_container} on {palette.accent_container}",
        TRAIL: f"{palette.on_surface} on {palette.bar}",
        COUNT: f"{palette.outline} on {palette.bar}",
    }


DEFAULT_STYLES: Final = styles_for(DEFAULT_PALETTE)

STYLE_NAMES: Final[dict[str, str]] = {
    name.removeprefix("tasktui."): name for name in DEFAULT_STYLES
}
PALETTE_NAMES: Final = tuple(field.name for field in fields(Palette))
GLYPH_NAMES: Final = tuple(field.name for field in fields(Glyphs))


def _amended[T](original: T, overrides: dict[str, str], known: tuple[str, ...]) -> T:
    for name in overrides:
        if name not in known:
            raise UnknownStyle(
                f"unknown name {name!r}; known names are: {', '.join(known)}"
            )
    return replace(original, **overrides)  # type: ignore[type-var]


def palette_from(overrides: dict[str, str] | None = None) -> Palette:
    """The palette, with any overrides applied.

    Raises:
        UnknownStyle: if an override names a colour the interface never uses,
            or gives something rich cannot read as a colour.
    """
    amended = _amended(DEFAULT_PALETTE, overrides or {}, PALETTE_NAMES)
    for name in PALETTE_NAMES:
        _checked(f"palette.{name}", getattr(amended, name))
    return amended


def glyphs_from(overrides: dict[str, str] | None = None) -> Glyphs:
    """The glyphs, with any overrides applied.

    Raises:
        UnknownStyle: if an override names a glyph the interface never draws.
    """
    return _amended(DEFAULT_GLYPHS, overrides or {}, GLYPH_NAMES)


def build_theme(
    overrides: dict[str, str] | None = None,
    palette: Palette | None = None,
) -> Theme:
    """The styles for a palette, with any individual style overridden.

    Raises:
        UnknownStyle: if an override names an unknown style, or is not
            something rich can read as a style.
    """
    styles = styles_for(palette or DEFAULT_PALETTE)
    for short, definition in (overrides or {}).items():
        full = STYLE_NAMES.get(short)
        if full is None:
            known = ", ".join(sorted(STYLE_NAMES))
            raise UnknownStyle(f"unknown style {short!r}; known styles are: {known}")
        styles[full] = _checked(f"theme.{short}", definition)
    return Theme(styles)


def _checked(where: str, definition: str) -> str:
    try:
        Style.parse(definition)
    except (StyleSyntaxError, ColorParseError) as error:
        raise UnknownStyle(f"{where}: {error}") from error
    return definition


@dataclass(frozen=True, slots=True)
class Look:
    """Everything about appearance that the view needs to draw a frame."""

    palette: Palette = DEFAULT_PALETTE
    glyphs: Glyphs = DEFAULT_GLYPHS

    @classmethod
    def standard(cls) -> Self:
        return cls()

    def theme(self, overrides: dict[str, str] | None = None) -> Theme:
        return build_theme(overrides, self.palette)
