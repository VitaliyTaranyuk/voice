"""Frozen / portable entrypoint for the Voice local API (no --reload)."""

from __future__ import annotations

import os
import sys
from pathlib import Path


def _exe_dir() -> Path:
    if getattr(sys, "frozen", False):
        return Path(sys.executable).resolve().parent
    return Path(__file__).resolve().parent


def _prepare_env(work_dir: Path) -> None:
    os.chdir(work_dir)
    # Prefer bundled secrets; fall back to .env for local sidecar tests.
    for name in ("runtime.env", ".env"):
        candidate = work_dir / name
        if candidate.is_file():
            os.environ.setdefault("VOICE_ENV_FILE", str(candidate))
            break

    local_app = os.environ.get("LOCALAPPDATA") or str(Path.home() / "AppData" / "Local")
    cache_root = Path(local_app) / "Voice"
    cache_root.mkdir(parents=True, exist_ok=True)
    os.environ.setdefault("HF_HOME", str(cache_root / "hf-cache"))
    os.environ.setdefault("VOICE_DATA_DIR", str(cache_root))


def main() -> None:
    work_dir = _exe_dir()
    _prepare_env(work_dir)

    import uvicorn

    uvicorn.run(
        "app.main:app",
        host="127.0.0.1",
        port=8787,
        reload=False,
        log_level="info",
    )


if __name__ == "__main__":
    main()
