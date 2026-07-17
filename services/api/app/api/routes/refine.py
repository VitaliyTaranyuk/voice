from fastapi import APIRouter, HTTPException

from app.application.refine_text import RefineTextUseCase
from app.schemas.refine import RefineRequest, RefineResponse

router = APIRouter()


@router.post("/ai/refine", response_model=RefineResponse)
async def refine_text(body: RefineRequest) -> RefineResponse:
    use_case = RefineTextUseCase()
    try:
        return await use_case.execute(body)
    except ValueError as exc:
        raise HTTPException(status_code=503, detail=str(exc)) from exc
