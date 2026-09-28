"""Signal aggregation primitives for the Acme analytics stack."""

from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime, timezone


@dataclass(frozen=True)
class Sample:
    """A single observed value and when it was captured."""

    name: str
    value: float
    captured_at: datetime


def sample(name: str, value: float) -> Sample:
    """Capture a sample stamped with the current UTC time."""
    return Sample(name=name, value=value, captured_at=datetime.now(timezone.utc))


def mean(samples: list[Sample]) -> float:
    """Return the arithmetic mean of the sample values."""
    if not samples:
        raise ValueError("mean() requires at least one sample")
    return sum(item.value for item in samples) / len(samples)


def summarize(samples: list[Sample]) -> dict[str, float]:
    """Return the count, mean, and spread of a set of samples."""
    if not samples:
        return {"count": 0.0, "mean": 0.0, "spread": 0.0}
    values = [item.value for item in samples]
    return {
        "count": float(len(values)),
        "mean": mean(samples),
        "spread": max(values) - min(values),
    }
