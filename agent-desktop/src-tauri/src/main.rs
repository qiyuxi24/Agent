#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    agent_desktop_lib::run();

    // 兜底清理：如果 CloseRequested 钩子未触发，此处确保子进程被回收
    // code-server 的 Child 是 std::process::Child，kill() 同步，不需要 async runtime
    eprintln!("[Agent] main() 退出，执行兜底清理...");
    // 注意：MCP 进程由 McpManager 管理，在 AppState 被 drop 时无法异步清理
    // code-server 是全局 static，此处可同步 kill
    if let Ok(rt) = tokio::runtime::Runtime::new() {
        rt.block_on(async {
            agent_desktop_lib::shutdown_code_server().await;
        });
    }
    eprintln!("[Agent] 清理完成");
}
