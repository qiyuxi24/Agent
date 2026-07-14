from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from contextlib import asynccontextmanager

from app.api.chat import router as chat_router


@asynccontextmanager
async def lifespan(app: FastAPI):
    print("🚀 Votek 后端代理已启动")
    print("   http://localhost:8000")
    print("   POST /chat - 流式对话 (SSE)")
    yield
    print("🛑 后端代理已关闭")


app = FastAPI(
    title="Votek Proxy",
    version="0.1.0",
    lifespan=lifespan,
)

app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

app.include_router(chat_router)


@app.get("/health")
async def health():
    return {"status": "ok"}
