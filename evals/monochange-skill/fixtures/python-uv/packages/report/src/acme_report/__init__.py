"""Render human-readable reports from Acme Insight samples."""

from __future__ import annotations

from acme_insight import Sample, summarize


def render(samples: list[Sample]) -> str:
    """Render a one-line summary of the provided samples."""
    stats = summarize(samples)
    return (
        f"samples={int(stats['count'])} "
        f"mean={stats['mean']:.3f} "
        f"spread={stats['spread']:.3f}"
    )


def render_lines(samples: list[Sample]) -> list[str]:
    """Render one line per sample followed by a summary line."""
    lines = [f"{item.name}: {item.value:.3f}" for item in samples]
    lines.append(render(samples))
    return lines
