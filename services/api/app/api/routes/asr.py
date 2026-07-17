from fastapi import APIRouter, File, Form, HTTPException, UploadFile

from app.adapters.asr import AsrError, AsrRouter
from app.schemas.asr import AsrResponse

router = APIRouter()


@router.post("/ai/asr", response_model=AsrResponse)
async def transcribe_audio(
    file: UploadFile = File(...),
    locale: str = Form(default="ru"),
) -> AsrResponse:
    audio = await file.read()
    filename = file.filename or "audio.wav"
    content_type = file.content_type or "audio/wav"
    language = (locale or "ru").split("-")[0].lower()
    try:
        return await AsrRouter().transcribe(
            audio=audio,
            filename=filename,
            content_type=content_type,
            language=language,
        )
    except AsrError as exc:
        raise HTTPException(status_code=503, detail=str(exc)) from exc
