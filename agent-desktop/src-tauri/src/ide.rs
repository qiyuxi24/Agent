//! IDE 编译器内核模块
//!
//! 提供代码执行、文件操作、终端等 IDE 核心能力：
//! - 代码编译/执行（Python、JavaScript、TypeScript、Rust、Go 等）
//! - 工作区文件读写 / 创建 / 删除 / 重命名
//! - 目录浏览 + 文件搜索
//! - 可用语言检测
//! - 终端命令执行（交互式/非交互式）

use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::mpsc;
use std::time::Duration;

/// 代码执行超时（秒）
const EXEC_TIMEOUT: u64 = 30;

// ===== 请求/响应类型 =====

#[derive(Debug, Deserialize)]
pub struct ExecuteRequest {
    /// 语言：python / javascript / typescript / rust / go / c / cpp / bash / ruby / php
    pub language: String,
    /// 源代码
    pub code: String,
    /// 可选：编译参数
    #[serde(default)]
    pub args: Vec<String>,
    /// 可选：标准输入
    #[serde(default)]
    pub stdin: String,
}

#[derive(Debug, Serialize)]
pub struct ExecuteResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub timed_out: bool,
    pub elapsed_ms: u64,
}

#[derive(Debug, Serialize)]
pub struct LanguageInfo {
    pub id: String,
    pub name: String,
    pub extension: String,
    pub available: bool,
    pub version: String,
}

#[derive(Debug, Serialize)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
}

#[derive(Debug, Serialize)]
pub struct FileInfo {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    #[serde(rename = "modified")]
    pub modified: String,
}

/// 终端执行结果
#[derive(Debug, Serialize)]
pub struct TerminalResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

// ===== 语言检测 =====

fn detect_languages() -> Vec<LanguageInfo> {
    let langs: Vec<(&str, &str, &str, &[&str])> = vec![
        ("python", "Python", "py", &["python3", "python", "py"]),
        ("javascript", "JavaScript", "js", &["node"]),
        ("typescript", "TypeScript", "ts", &["ts-node", "npx"]),
        ("rust", "Rust", "rs", &["rustc"]),
        ("go", "Go", "go", &["go"]),
        ("c", "C", "c", &["gcc"]),
        ("cpp", "C++", "cpp", &["g++", "clang++"]),
        ("ruby", "Ruby", "rb", &["ruby"]),
        ("php", "PHP", "php", &["php"]),
        ("bash", "Bash", "sh", &["bash"]),
    ];

    langs
        .into_iter()
        .map(|(id, name, ext, commands)| {
            let (available, version) = detect_command(commands);
            LanguageInfo {
                id: id.to_string(),
                name: name.to_string(),
                extension: ext.to_string(),
                available,
                version,
            }
        })
        .collect()
}

fn detect_command(commands: &[&str]) -> (bool, String) {
    for cmd in commands {
        if let Ok(output) = std::process::Command::new(*cmd)
            .arg("--version")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
        {
            let v = String::from_utf8_lossy(&output.stdout)
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
            if !v.is_empty() {
                return (true, v);
            }
        }
        if let Ok(output) = std::process::Command::new(*cmd)
            .arg("-v")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
        {
            let v = String::from_utf8_lossy(&output.stdout)
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
            let v = if v.is_empty() {
                String::from_utf8_lossy(&output.stderr)
                    .lines()
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_string()
            } else {
                v
            };
            if !v.is_empty() {
                return (true, v);
            }
        }
    }
    (false, "not installed".to_string())
}

// ===== 代码执行 =====

#[tauri::command]
pub async fn ide_execute_code(request: ExecuteRequest) -> Result<ExecuteResult, String> {
    let start = std::time::Instant::now();
    let lang = request.language.clone();
    let code = request.code.clone();
    let args = request.args.clone();
    let stdin = request.stdin.clone();

    tokio::task::spawn_blocking(move || match lang.as_str() {
        "python" => run_python(&code, &args, &stdin, start),
        "javascript" => run_script("node", &["-e"], &code, &stdin, "Node.js", start),
        "typescript" => run_typescript(&code, &stdin, start),
        "rust" => run_compiled("rustc", "temp.rs", "temp_rs", &[], &code, &stdin, start),
        "go" => run_compiled_go(&code, &stdin, start),
        "c" => run_compiled("gcc", "temp.c", "temp_c", &[], &code, &stdin, start),
        "cpp" => run_compiled("g++", "temp.cpp", "temp_cpp", &[], &code, &stdin, start),
        "bash" => run_bash(&code, &stdin, start),
        "ruby" => run_script("ruby", &[], &code, &stdin, "Ruby", start),
        "php" => run_script("php", &["-r"], &code, &stdin, "PHP", start),
        _ => Err(format!("不支持的语言: {}", lang)),
    })
    .await
    .map_err(|e| format!("执行线程错误: {}", e))?
}

// ---- 脚本语言 ----

fn run_python(code: &str, args: &[String], stdin: &str, start: std::time::Instant) -> Result<ExecuteResult, String> {
    let mut cmd = std::process::Command::new("python3");
    for a in args {
        cmd.arg(a);
    }
    cmd.arg("-c").arg(code)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let child = cmd.spawn().or_else(|_| {
        let mut c = std::process::Command::new("python");
        c.arg("-c").arg(code)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        c.spawn()
    }).map_err(|e| format!("Python 执行失败: {}", e))?;

    wait_timeout(child, stdin, start)
}

fn run_script(cmd: &str, extra: &[&str], code: &str, stdin: &str, label: &str, start: std::time::Instant) -> Result<ExecuteResult, String> {
    let mut c = std::process::Command::new(cmd);
    for a in extra {
        c.arg(a);
    }
    let child = c.arg(code)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("{} 执行失败: {}", label, e))?;

    wait_timeout(child, stdin, start)
}

fn run_typescript(code: &str, stdin: &str, start: std::time::Instant) -> Result<ExecuteResult, String> {
    let child = std::process::Command::new("ts-node")
        .args(["-e", code])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .or_else(|_| {
            std::process::Command::new("npx")
                .args(["ts-node", "-e", code])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
        })
        .map_err(|e| format!("TypeScript 执行失败（需要 ts-node）: {}", e))?;

    wait_timeout(child, stdin, start)
}

fn run_bash(code: &str, stdin: &str, start: std::time::Instant) -> Result<ExecuteResult, String> {
    let (shell, flag) = if cfg!(windows) {
        ("powershell", "-Command")
    } else {
        ("bash", "-c")
    };
    let child = std::process::Command::new(shell)
        .arg(flag)
        .arg(code)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("{} 执行失败: {}", shell, e))?;

    wait_timeout(child, stdin, start)
}

// ---- 编译型语言 ----

fn run_compiled(compiler: &str, src_name: &str, bin_base: &str, extra_args: &[&str], code: &str, stdin: &str, start: std::time::Instant) -> Result<ExecuteResult, String> {
    let (_, src_file, bin_file) = setup_temp(src_name, bin_base)?;
    std::fs::write(&src_file, code).map_err(|e| format!("写入临时文件失败: {}", e))?;

    let mut cc = std::process::Command::new(compiler);
    cc.arg(&src_file).arg("-o").arg(&bin_file);
    for a in extra_args {
        cc.arg(a);
    }
    let compile_out = cc.stdout(Stdio::piped()).stderr(Stdio::piped()).output()
        .map_err(|e| format!("{} 编译失败: {}", compiler, e))?;

    if !compile_out.status.success() {
        let err = String::from_utf8_lossy(&compile_out.stderr).to_string();
        let elapsed = start.elapsed().as_millis() as u64;
        let _ = std::fs::remove_file(&src_file);
        return Ok(ExecuteResult { stdout: String::new(), stderr: format!("编译错误:\n{}", err), exit_code: 1, timed_out: false, elapsed_ms: elapsed });
    }

    let child = std::process::Command::new(&bin_file)
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())
        .spawn().map_err(|e| format!("运行失败: {}", e))?;

    let res = wait_timeout(child, stdin, start);
    let _ = std::fs::remove_file(&src_file);
    let _ = std::fs::remove_file(&bin_file);
    res
}

fn run_compiled_go(code: &str, stdin: &str, start: std::time::Instant) -> Result<ExecuteResult, String> {
    let (_, src_file, bin_file) = setup_temp("temp.go", "temp_go")?;
    std::fs::write(&src_file, code).map_err(|e| format!("写入临时文件失败: {}", e))?;

    let compile_out = std::process::Command::new("go")
        .args(["build", "-o"])
        .arg(&bin_file).arg(&src_file)
        .stdout(Stdio::piped()).stderr(Stdio::piped())
        .output().map_err(|e| format!("go build 失败: {}", e))?;

    if !compile_out.status.success() {
        let err = String::from_utf8_lossy(&compile_out.stderr).to_string();
        let elapsed = start.elapsed().as_millis() as u64;
        let _ = std::fs::remove_file(&src_file);
        return Ok(ExecuteResult { stdout: String::new(), stderr: format!("编译错误:\n{}", err), exit_code: 1, timed_out: false, elapsed_ms: elapsed });
    }

    let child = std::process::Command::new(&bin_file)
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())
        .spawn().map_err(|e| format!("运行失败: {}", e))?;

    let res = wait_timeout(child, stdin, start);
    let _ = std::fs::remove_file(&src_file);
    let _ = std::fs::remove_file(&bin_file);
    res
}

// ---- 核心：带超时的进程等待 ----

fn setup_temp(src_name: &str, bin_base: &str) -> Result<(PathBuf, PathBuf, PathBuf), String> {
    let tmp_dir = std::env::temp_dir().join("agent-ide");
    std::fs::create_dir_all(&tmp_dir).map_err(|e| format!("创建临时目录失败: {}", e))?;
    let src_file = tmp_dir.join(src_name);
    let ext = if cfg!(windows) { ".exe" } else { "" };
    let bin_file = tmp_dir.join(format!("{}{}", bin_base, ext));
    Ok((tmp_dir, src_file, bin_file))
}

fn wait_timeout(mut child: std::process::Child, stdin_text: &str, start: std::time::Instant) -> Result<ExecuteResult, String> {
    if !stdin_text.is_empty() {
        if let Some(mut sin) = child.stdin.take() {
            let _ = sin.write_all(stdin_text.as_bytes());
        }
    }

    let pid = child.id();

    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = child.wait_with_output();
        let _ = tx.send(result);
    });

    match rx.recv_timeout(Duration::from_secs(EXEC_TIMEOUT)) {
        Ok(Ok(output)) => {
            let elapsed = start.elapsed().as_millis() as u64;
            Ok(ExecuteResult {
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                exit_code: output.status.code().unwrap_or(-1),
                timed_out: false,
                elapsed_ms: elapsed,
            })
        }
        Ok(Err(e)) => {
            let elapsed = start.elapsed().as_millis() as u64;
            Ok(ExecuteResult {
                stdout: String::new(),
                stderr: format!("进程错误: {}", e),
                exit_code: -1,
                timed_out: false,
                elapsed_ms: elapsed,
            })
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {
            kill_process(pid);
            let elapsed = start.elapsed().as_millis() as u64;
            Ok(ExecuteResult {
                stdout: String::new(),
                stderr: format!("执行超时（{}秒），已终止进程", EXEC_TIMEOUT),
                exit_code: -1,
                timed_out: true,
                elapsed_ms: elapsed,
            })
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            let elapsed = start.elapsed().as_millis() as u64;
            Ok(ExecuteResult {
                stdout: String::new(),
                stderr: "进程异常终止".to_string(),
                exit_code: -1,
                timed_out: false,
                elapsed_ms: elapsed,
            })
        }
    }
}

fn kill_process(pid: u32) {
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F", "/T"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
    }
    #[cfg(not(windows))]
    {
        unsafe { libc::kill(pid as i32, libc::SIGKILL); }
    }
}

// ===== 可用语言检测 =====

#[tauri::command]
pub async fn ide_get_languages() -> Result<Vec<LanguageInfo>, String> {
    tokio::task::spawn_blocking(detect_languages)
        .await
        .map_err(|e| format!("检测语言失败: {}", e))
}

// ===== 文件操作 =====

#[tauri::command]
pub async fn ide_read_file(path: String) -> Result<String, String> {
    tokio::fs::read_to_string(&path)
        .await
        .map_err(|e| format!("读取文件失败: {}", e))
}

#[tauri::command]
pub async fn ide_write_file(path: String, content: String) -> Result<(), String> {
    if let Some(parent) = PathBuf::from(&path).parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("创建目录失败: {}", e))?;
    }
    tokio::fs::write(&path, content)
        .await
        .map_err(|e| format!("写入文件失败: {}", e))
}

/// 创建文件或文件夹
#[tauri::command]
pub async fn ide_create_file(path: String, is_dir: bool) -> Result<(), String> {
    let p = PathBuf::from(&path);
    if is_dir {
        tokio::fs::create_dir_all(&p)
            .await
            .map_err(|e| format!("创建目录失败: {}", e))?;
    } else {
        if let Some(parent) = p.parent() {
            if !parent.as_os_str().is_empty() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(|e| format!("创建父目录失败: {}", e))?;
            }
        }
        if !p.exists() {
            tokio::fs::write(&p, "")
                .await
                .map_err(|e| format!("创建文件失败: {}", e))?;
        }
    }
    Ok(())
}

/// 删除文件或文件夹（文件夹递归删除）
#[tauri::command]
pub async fn ide_delete_file(path: String, is_dir: bool) -> Result<(), String> {
    let p = PathBuf::from(&path);
    if !p.exists() {
        return Err("文件/目录不存在".to_string());
    }
    if is_dir {
        tokio::fs::remove_dir_all(&p)
            .await
            .map_err(|e| format!("删除目录失败: {}", e))?;
    } else {
        tokio::fs::remove_file(&p)
            .await
            .map_err(|e| format!("删除文件失败: {}", e))?;
    }
    Ok(())
}

/// 重命名文件或文件夹
#[tauri::command]
pub async fn ide_rename_file(old_path: String, new_path: String) -> Result<(), String> {
    tokio::fs::rename(&old_path, &new_path)
        .await
        .map_err(|e| format!("重命名失败: {}", e))
}

/// 移动文件（复制 + 删除）
#[tauri::command]
pub async fn ide_move_file(source: String, destination: String) -> Result<(), String> {
    let src = PathBuf::from(&source);
    let dst = PathBuf::from(&destination);
    if !src.exists() {
        return Err("源文件不存在".to_string());
    }
    if src.is_dir() {
        // 目录：递归拷贝
        copy_dir_recursive(&src, &dst)
            .map_err(|e| format!("移动目录失败: {}", e))?;
        std::fs::remove_dir_all(&src)
            .map_err(|e| format!("删除源目录失败: {}", e))?;
    } else {
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("创建目标目录失败: {}", e))?;
        }
        std::fs::rename(&src, &dst)
            .map_err(|e| format!("移动文件失败: {}", e))?;
    }
    Ok(())
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn ide_list_dir(path: String, show_hidden: Option<bool>) -> Result<Vec<FileEntry>, String> {
    let show = show_hidden.unwrap_or(false);
    let mut entries = tokio::fs::read_dir(&path)
        .await
        .map_err(|e| format!("读取目录失败: {}", e))?;

    let mut result = Vec::new();
    let mut entry_results = Vec::new();

    while let Ok(Some(entry)) = entries.next_entry().await {
        entry_results.push(entry);
    }

    for entry in entry_results {
        let name = entry.file_name().to_string_lossy().to_string();
        if !show && (name.starts_with('.') || name == "node_modules" || name == "target") {
            continue;
        }
        let path = entry.path().to_string_lossy().to_string();
        let is_dir = entry.file_type().await.map(|t| t.is_dir()).unwrap_or(false);
        let size = if is_dir {
            0
        } else {
            entry.metadata().await.map(|m| m.len()).unwrap_or(0)
        };

        result.push(FileEntry { name, path, is_dir, size });
    }

    result.sort_by(|a, b| {
        b.is_dir.cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    Ok(result)
}

/// 文件搜索：在当前目录中递归搜索包含关键字的文件
#[tauri::command]
pub async fn ide_search_files(dir: String, query: String, case_sensitive: Option<bool>) -> Result<Vec<String>, String> {
    let case = case_sensitive.unwrap_or(false);
    let query_lower = if case { query.clone() } else { query.to_lowercase() };
    let dir = PathBuf::from(dir);

    let result = tokio::task::spawn_blocking(move || {
        let mut matches = Vec::new();
        search_dir(&dir, &query_lower, case, &mut matches);
        matches
    })
    .await
    .map_err(|e| format!("搜索线程错误: {}", e))?;

    Ok(result)
}

fn search_dir(dir: &Path, query: &str, case: bool, results: &mut Vec<String>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            // 跳过隐藏文件和常见忽略目录
            if name.starts_with('.') || name == "node_modules" || name == "target" || name == "__pycache__" {
                continue;
            }
            let path = entry.path();
            if path.is_dir() {
                search_dir(&path, query, case, results);
            } else {
                // 在文件名中搜索
                let name_check = if case { name.clone() } else { name.to_lowercase() };
                if name_check.contains(query) {
                    results.push(path.to_string_lossy().to_string());
                    continue;
                }
                // 在文件内容中搜索（限制文件大小 1MB 以下）
                if let Ok(meta) = std::fs::metadata(&path) {
                    if meta.len() < 1_000_000 {
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            let content_check = if case { content } else { content.to_lowercase() };
                            if content_check.contains(query) {
                                results.push(path.to_string_lossy().to_string());
                            }
                        }
                    }
                }
            }
        }
    }
}

/// 获取文件详细信息
#[tauri::command]
pub fn ide_get_file_info(path: String) -> Result<FileInfo, String> {
    let p = PathBuf::from(&path);
    let meta = p.metadata().map_err(|e| format!("读取文件信息失败: {}", e))?;
    let modified = meta.modified()
        .ok()
        .and_then(|t| {
            let duration = t.duration_since(std::time::UNIX_EPOCH).ok()?;
            let secs = duration.as_secs();
            // 简单格式化
            let dt = chrono_lite(secs);
            Some(dt)
        })
        .unwrap_or_default();

    Ok(FileInfo {
        name: p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
        path: path,
        is_dir: meta.is_dir(),
        size: meta.len(),
        modified,
    })
}

/// 极简时间格式化（避免引入 chrono 依赖）
fn chrono_lite(secs: u64) -> String {
    let days = secs / 86400;
    let remaining = secs % 86400;
    let hours = remaining / 3600;
    let minutes = (remaining % 3600) / 60;
    let seconds = remaining % 60;
    // 从 epoch 计算年月日（简化版）
    let total_days = days as i64;
    let (y, m, d) = days_to_date(total_days);
    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", y, m, d, hours, minutes, seconds)
}

fn days_to_date(total_days: i64) -> (i64, u32, u32) {
    let mut days = total_days;
    // 基准：1970-01-01
    let mut year: i64 = 1970;
    loop {
        let days_in_year = if is_leap(year) { 366 } else { 365 };
        if days < days_in_year { break; }
        days -= days_in_year;
        year += 1;
    }
    let months_days = if is_leap(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut month: u32 = 1;
    for &md in &months_days {
        if days < md { break; }
        days -= md;
        month += 1;
    }
    (year, month, (days + 1) as u32)
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0)
}

// ===== 工作区管理 =====

#[tauri::command]
pub async fn ide_get_workspace() -> Result<String, String> {
    Ok(std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default())
}

/// 切换工作目录
#[tauri::command]
pub async fn ide_set_workspace(path: String) -> Result<String, String> {
    let p = PathBuf::from(&path);
    if !p.exists() {
        return Err(format!("路径不存在: {}", path));
    }
    if !p.is_dir() {
        return Err(format!("路径不是目录: {}", path));
    }
    std::env::set_current_dir(&p)
        .map_err(|e| format!("切换工作目录失败: {}", e))?;
    Ok(p.canonicalize()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or(path))
}

/// 打开系统文件对话框选择目录
/// TODO: 接入 tauri-plugin-dialog 实现原生目录选择器
#[allow(dead_code)]
#[tauri::command]
pub async fn ide_pick_directory() -> Result<Option<String>, String> {
    Err("暂未实现，请直接输入路径".to_string())
}

// ===== 终端命令执行 =====

/// 在工作目录下执行终端命令（非交互式，带超时）
#[tauri::command]
pub async fn ide_terminal_exec(command: String, cwd: Option<String>) -> Result<TerminalResult, String> {
    let cwd_path = cwd.map(PathBuf::from);

    tokio::task::spawn_blocking(move || {
        let (shell, flag) = if cfg!(windows) {
            ("powershell", "-Command")
        } else {
            ("bash", "-c")
        };

        let mut cmd = std::process::Command::new(shell);
        cmd.arg(flag).arg(&command)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if let Some(ref dir) = cwd_path {
            cmd.current_dir(dir);
        }

        let output = cmd.output()
            .map_err(|e| format!("命令执行失败: {}", e))?;

        Ok(TerminalResult {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code().unwrap_or(-1),
        })
    })
    .await
    .map_err(|e| format!("终端线程错误: {}", e))?
}
