# -*- mode: python ; coding: utf-8 -*-
# PyInstaller onedir build for Voice local API sidecar.
# Run via: scripts/build-api-sidecar.ps1

from PyInstaller.utils.hooks import collect_all

datas: list = []
binaries: list = []
hiddenimports: list = [
    "uvicorn.logging",
    "uvicorn.loops",
    "uvicorn.loops.auto",
    "uvicorn.protocols",
    "uvicorn.protocols.http",
    "uvicorn.protocols.http.auto",
    "uvicorn.protocols.websockets",
    "uvicorn.protocols.websockets.auto",
    "uvicorn.lifespan",
    "uvicorn.lifespan.on",
    "app.main",
    "app.api.router",
    "app.api.routes.asr",
    "app.api.routes.health",
    "app.api.routes.refine",
    "app.adapters.asr",
    "app.adapters.deepseek",
    "app.application.refine_text",
    "app.core.config",
    "app.domain.guardrails",
    "app.schemas.asr",
    "app.schemas.health",
    "app.schemas.refine",
]

for package in (
    "faster_whisper",
    "ctranslate2",
    "tokenizers",
    "huggingface_hub",
    "av",
    "onnxruntime",
):
    try:
        d, b, h = collect_all(package)
        datas += d
        binaries += b
        hiddenimports += h
    except Exception:
        pass

# CUDA намеренно НЕ включается в сайдкар: библиотеки nvidia-* занимали 1984 МБ
# из 2236 МБ сборки, то есть 89% веса раздаваемого установщика ради ускорения на
# машинах с картой NVIDIA. Собранный с ними инсталлятор пришлось бы качать
# гигабайтом.
#
# Это безопасно: у ctranslate2.dll нет статического импорта CUDA (проверено по
# таблице импорта PE — обязательные только KERNEL32, рантайм MSVC и
# libiomp5md.dll), CUDA грузится лениво. Неудачную попытку ловит
# LocalWhisperAsrAdapter._ensure_model и переходит на cpu/int8.
#
# Для локальной разработки CUDA остаётся: зависимости в pyproject.toml не
# тронуты, `uv sync` ставит их как раньше — исключение действует только на
# замороженную сборку.
def _is_cuda_lib(dest: str) -> bool:
    low = dest.lower().replace("\\", "/")
    return "nvidia/" in low or any(
        part in low for part in ("cublas", "cudnn", "cudart", "nvrtc", "cufft", "curand")
    )


binaries = [item for item in binaries if not _is_cuda_lib(item[0])]

a = Analysis(
    ["sidecar_main.py"],
    pathex=["."],
    binaries=binaries,
    datas=datas,
    hiddenimports=hiddenimports,
    hookspath=[],
    hooksconfig={},
    runtime_hooks=[],
    excludes=[],
    noarchive=False,
    optimize=0,
)
# Analysis сканирует зависимости сама и может подтянуть те же библиотеки заново,
# уже мимо списка выше. Фильтр после неё — тот, который реально определяет
# содержимое сборки.
a.binaries = [item for item in a.binaries if not _is_cuda_lib(item[0])]

pyz = PYZ(a.pure)

exe = EXE(
    pyz,
    a.scripts,
    [],
    exclude_binaries=True,
    name="voice-api",
    debug=False,
    bootloader_ignore_signals=False,
    strip=False,
    upx=False,
    console=False,
    disable_windowed_traceback=False,
    argv_emulation=False,
    target_arch=None,
    codesign_identity=None,
    entitlements_file=None,
)
coll = COLLECT(
    exe,
    a.binaries,
    a.datas,
    strip=False,
    upx=False,
    upx_exclude=[],
    name="voice-api",
)
