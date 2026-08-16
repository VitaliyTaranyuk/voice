# -*- mode: python ; coding: utf-8 -*-
# PyInstaller onedir build for Voice local API sidecar.
# Run via: scripts/build-api-sidecar.ps1

from PyInstaller.utils.hooks import collect_all, collect_dynamic_libs

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

for package in (
    "nvidia.cublas",
    "nvidia.cuda_runtime",
    "nvidia.cudnn",
    "nvidia.cuda_nvrtc",
):
    try:
        binaries += collect_dynamic_libs(package)
    except Exception:
        pass

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
