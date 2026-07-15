from pydantic_settings import BaseSettings


class Settings(BaseSettings):
    # 默认值仅作 fallback，实际配置由前端通过请求头传入
    llm_api_base: str = "https://api.openai.com/v1"
    llm_model: str = "gpt-4o"
    log_level: str = "info"

    model_config = {"env_file": ".env", "env_file_encoding": "utf-8"}


settings = Settings()
