"""Frozen / portable entrypoint for the Voice local API (no --reload)."""

from __future__ import annotations

import os
import sys
from pathlib import Path


def _exe_dir() -> Path:
    if getattr(sys, "frozen", False):
        return Path(sys.executable).resolve().parent
    return Path(__file__).resolve().parent


def _open_log(cache_root: Path):
    """Open the sidecar log, degrading to NUL rather than failing to start."""
    for target in (cache_root / "logs" / "voice-api.log", Path(os.devnull)):
        try:
            target.parent.mkdir(parents=True, exist_ok=True)
            # Line buffered: whatever killed the process must already be on disk.
            # Truncating: one process owns port 8787, so the last run is what
            # anyone debugging needs, and an append-only file nobody rotates grows
            # without limit. The handle deliberately outlives this function.
            return open(target, "w", buffering=1, encoding="utf-8", errors="replace")
        except OSError:
            continue
    return None


def _bind_streams(cache_root: Path) -> None:
    """Give the frozen sidecar streams it can actually write to.

    PyInstaller builds this with `console=False` (see voice_api.spec), and a
    windowed process that inherited no console has `sys.stdout is None`. Every
    consumer then dies: uvicorn's default log config builds `DefaultFormatter`,
    which asks `sys.stdout.isatty()`, and the process fails before it ever binds
    the port. Measured on the same executable — launched without a console it
    dies with "Unable to configure formatter 'default'", launched with one it
    serves 8787. That is why dictation worked from `scripts/start-voice.cmd`
    and not from the Start menu shortcut the installer creates, which points
    straight at voice-desktop.exe.

    The streams are made real instead of silencing the one caller that tripped
    over them: the model download writes a tqdm bar to stderr and would fail the
    same way, as would any `print` inside a dependency.
    """
    if sys.stdout is not None and sys.stderr is not None:
        return
    stream = _open_log(cache_root)
    if stream is None:
        return
    if sys.stdout is None:
        sys.stdout = stream
    if sys.stderr is None:
        sys.stderr = stream


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
    _bind_streams(cache_root)
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
