from fastapi import APIRouter

from app.api.routes import asr, health, refine

api_router = APIRouter()
api_router.include_router(health.router, tags=["health"])
api_router.include_router(refine.router, tags=["ai"])
api_router.include_router(asr.router, tags=["ai"])
