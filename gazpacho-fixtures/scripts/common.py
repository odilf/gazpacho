from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from pathlib import Path
from typing import Literal

# Paths are resolved from this file's location, so the script works regardless
# of the working directory it is invoked from (e.g. a nextest setup script).
REPO_ROOT = Path(__file__).resolve().parents[2]
FIXTURES_DIR = REPO_ROOT / "target" / "gazpacho-fixtures"

type Json = str | int | Sequence[Json] | Mapping[str, Json]
    
@dataclass(eq=False)
class Video:
    name: str
    category: str
    failed: Literal[False] | str
    meta: dict[str, Json]
    forced_path: None | Path = None

    def path(self) -> Path:
        return FIXTURES_DIR / self.category / self.name if self.forced_path is None else self.forced_path

    def to_json(self) -> Json:
        # The manifest is grouped by kind (`generate.py` writes per-kind
        # arrays), so the entry carries the kind's own fields instead of a
        # heterogeneous `meta`: synthetic videos inline their spec, chromium
        # entries flatten their per-file annotation.
        entry: dict[str, Json] = {
            "name": self.name,
            "path": str(self.path()),
            **self.meta
        }

        if self.failed is not False:
            entry["failed"] = self.failed

        return entry 
