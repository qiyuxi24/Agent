use serde::Serialize;

/// code-server 运行状态
#[derive(Debug, Clone, Serialize)]
pub struct CodeServerStatus {
    pub installed: bool,
    pub running: bool,
    pub port: u16,
    pub workspace: String,
    pub url: String,
    pub version: String,
    /// 最近一次错误信息（启动失败/进程崩溃等），无错误时为 None
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// IDE 就绪事件（后端通知前端 code-server 已可访问）
#[derive(Debug, Clone, Serialize)]
pub struct IdeReadyEvent {
    pub url: String,
    pub port: u16,
    /// 失败原因（url 为空时表示启动失败，此字段含错误详情）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
