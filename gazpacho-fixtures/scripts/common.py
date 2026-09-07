from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from enum import StrEnum
from pathlib import Path
from typing import Literal

# Paths are resolved from this file's location, so the script works regardless
# of the working directory it is invoked from (e.g. a nextest setup script).
REPO_ROOT = Path(__file__).resolve().parents[2]
FIXTURES_DIR = REPO_ROOT / "target" / "gazpacho-fixtures"

type Json = None | str | int | Sequence[Json] | Mapping[str, Json]


class Category(StrEnum):
    SYNTHETIC = "synthetic"
    DERIVED = "derived"
    CHROMIUM = "chromium"
    REALISTIC = "realistic"


@dataclass(frozen=True)
class Video:
    name: str
    category: Category
    failed: Literal[False] | str
    meta: dict[str, Json]
    # Estimated decode work (width x height x frame count). 0 for failed videos.
    cost: int = 0
    forced_path: None | Path = None

    def path(self) -> Path:
        return FIXTURES_DIR / self.category / self.name if self.forced_path is None else self.forced_path

    def to_json(self) -> dict[str, Json]:
        entry: dict[str, Json] = {
            "name": self.name,
            "category": self.category,
            "path": str(self.path()),
            "cost": self.cost,
            **self.meta,
        }

        if self.failed is not False:
            entry["failed"] = self.failed

        return entry 
