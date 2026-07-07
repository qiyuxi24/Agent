from fastapi import APIRouter, HTTPException, Request
from fastapi.responses import StreamingResponse
from pydantic import BaseModel
from typing import List, Dict, Any

from app.services.llm_service import stream_chat_completion

router = APIRouter()


class ChatRequest(BaseModel):
    messages: List[Dict[str, str]]


@router.post("/chat")
async def chat(request: Request, body: ChatRequest):
    """对话接口，从请求头获取配置，流式返回 SSE"""
    api_key = request.headers.get("X-Api-Key", "").strip()
    api_base = request.headers.get("X-Api-Base", "").strip()
    model = request.headers.get("X-Model", "").strip()

    if not api_key:
        raise HTTPException(status_code=401, detail="请在设置中配置 API Key")

    if not api_base:
        api_base = "https://api.openai.com/v1"
    if not model:
        model = "gpt-4o"

    async def generate():
        try:
            async for chunk in stream_chat_completion(
                api_base=api_base,
                api_key=api_key,
                model=model,
                messages=body.messages,
            ):
                yield chunk
        except Exception as e:
            error_data = {
                "error": {"message": str(e), "type": "proxy_error"}
            }
            yield f"data: {__import__('json').dumps(error_data)}\n\n"
            yield "data: [DONE]\n\n"

    return StreamingResponse(
        generate(),
        media_type="text/event-stream",
        headers={
            "Cache-Control": "no-cache",
            "Connection": "keep-alive",
            "X-Accel-Buffering": "no",
        },
    )
