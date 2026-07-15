//! 沙箱执行环境 — 完全去耦合模块
//!
//! 职责：
//! - 管理隔离的沙箱目录（每个沙箱 = 一个本地文件夹）
//! - 提供安全的文件读写 / 目录列表 / 命令执行
//! - 路径逃逸检测：所有操作限制在沙箱根目录内，防止 AI 越权访问系统文件
//!
//! 设计原则：
//! - 零外部依赖（除 std + tokio）：可独立编译、独立测试
//! - 不关心 "工作空间" 概念：只接受一个 root_path 做沙箱根
//! - 上层（workspace 管理 / Tauri 命令）负责决定何时创建/销毁沙箱

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

// ── 错误类型 ──

#[derive(Debug)]
pub enum SandboxError {
    NotFound(String),
    PathEscape { requested: String, root: String },
    Io(std::io::Error),
    NotADirectory(String),
}

impl std::fmt::Display for SandboxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SandboxError::NotFound(id) => write!(f, "沙箱不存在: {id}"),
            SandboxError::PathEscape { requested, root } => {
                write!(f, "路径越界: {requested} 超出了沙箱根目录 {root}")
            }
            SandboxError::Io(e) => write!(f, "I/O 错误: {e}"),
            SandboxError::NotADirectory(p) => write!(f, "路径不是目录: {p}"),
        }
    }
}

impl From<std::io::Error> for SandboxError {
    fn from(e: std::io::Error) -> Self {
        SandboxError::Io(e)
    }
}

// ── 数据结构 ──

/// 单个沙箱实例
#[derive(Debug, Clone)]
pub struct Sandbox {
    pub id: String,
    /// 沙箱根目录（用户选择的文件夹路径）
    pub root: PathBuf,
    /// 沙箱内部数据目录（.votek-sandbox/），存放 AI 缓存/状态
    pub meta_dir: PathBuf,
    pub created_at: Instant,
}

/// 沙箱信息（给前端展示）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxInfo {
    pub id: String,
    pub root: String,
    /// 根目录显示名（路径最后一段）
    pub name: String,
}

/// 文件信息（给前端展示）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
}

// ── SandboxManager ──

pub struct SandboxManager {
    sandboxes: HashMap<String, Sandbox>,
}

impl SandboxManager {
    pub fn new() -> Self {
        Self { sandboxes: HashMap::new() }
    }

    // ── 生命周期 ──

    /// 创建新沙箱
    /// - root: 用户选择的本地文件夹路径（必须是已存在的目录）
    /// - 返回 sandbox_id
    pub fn create(&mut self, id: &str, name: &str, root: &Path) -> Result<SandboxInfo, SandboxError> {
        if !root.exists() || !root.is_dir() {
            return Err(SandboxError::NotADirectory(root.display().to_string()));
        }

        let meta_dir = root.join(".votek-sandbox");
        std::fs::create_dir_all(&meta_dir)?;

        let sandbox = Sandbox {
            id: id.to_string(),
            root: root.to_path_buf(),
            meta_dir,
            created_at: Instant::now(),
        };
        let info = SandboxInfo {
            id: id.to_string(),
            root: root.display().to_string(),
            name: name.to_string(),
        };
        self.sandboxes.insert(id.to_string(), sandbox);
        Ok(info)
    }

    /// 删除沙箱（不删除用户文件夹，仅清理内部元数据）
    pub fn remove(&mut self, id: &str) -> Result<(), SandboxError> {
        let sb = self.sandboxes.remove(id).ok_or_else(|| SandboxError::NotFound(id.to_string()))?;
        // 清理 .votek-sandbox 元数据目录（用户文件不动）
        if sb.meta_dir.exists() {
            std::fs::remove_dir_all(&sb.meta_dir)?;
        }
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&Sandbox> {
        self.sandboxes.get(id)
    }

    pub fn list(&self) -> Vec<SandboxInfo> {
        self.sandboxes.iter().map(|(_, sb)| SandboxInfo {
            id: sb.id.clone(),
            root: sb.root.display().to_string(),
            name: sb.root.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| sb.root.display().to_string()),
        }).collect()
    }

    // ── 路径安全验证 ──

    /// 将相对路径解析为沙箱内的绝对路径，并验证不越界
    /// `relative` 为空或 "." 时返回根目录
    fn resolve_safe(&self, sandbox_id: &str, relative: &str) -> Result<PathBuf, SandboxError> {
        let sb = self.sandboxes.get(sandbox_id)
            .ok_or_else(|| SandboxError::NotFound(sandbox_id.to_string()))?;

        let clean = relative.trim_start_matches(&['/', '\\']);
        let resolved = if clean.is_empty() || clean == "." {
            sb.root.clone()
        } else {
            // canonicalize 会解析 .. 和符号链接，得到真实路径
            let joined = sb.root.join(clean);
            match joined.canonicalize() {
                Ok(canon) => canon,
                Err(_) => {
                    // 路径不存在时，手动检测 .. 逃逸
                    let root_canon = sb.root.canonicalize().unwrap_or_else(|_| sb.root.clone());
                    // 构建绝对路径并检查前缀
                    let abs = if joined.is_absolute() { joined.clone() } else {
                        std::env::current_dir().unwrap_or_default().join(&joined)
                    };
                    // 简单检查：拼接后的路径是否以 root 开头
                    let root_str = root_canon.display().to_string();
                    let abs_str = abs.display().to_string();
                    if !abs_str.starts_with(&root_str) {
                        return Err(SandboxError::PathEscape {
                            requested: relative.to_string(),
                            root: root_str,
                        });
                    }
                    joined
                }
            }
        };

        let root_canon = sb.root.canonicalize().unwrap_or_else(|_| sb.root.clone());
        if !resolved.starts_with(&root_canon) {
            return Err(SandboxError::PathEscape {
                requested: relative.to_string(),
                root: root_canon.display().to_string(),
            });
        }
        Ok(resolved)
    }

    // ── 文件操作 ──

    /// 读取沙箱内的文件内容
    pub async fn read_file(&self, sandbox_id: &str, relative_path: &str) -> Result<String, SandboxError> {
        let path = self.resolve_safe(sandbox_id, relative_path)?;
        if !path.is_file() {
            return Err(SandboxError::NotFound(format!("文件不存在: {}", path.display())));
        }
        let content = tokio::fs::read_to_string(&path).await?;
        Ok(content)
    }

    /// 写入文件到沙箱内
    pub async fn write_file(&self, sandbox_id: &str, relative_path: &str, content: &str) -> Result<(), SandboxError> {
        let path = self.resolve_safe(sandbox_id, relative_path)?;
        // 确保父目录存在
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&path, content).await?;
        Ok(())
    }

    /// 删除沙箱内的文件或空目录
    pub async fn delete_file(&self, sandbox_id: &str, relative_path: &str) -> Result<(), SandboxError> {
        let path = self.resolve_safe(sandbox_id, relative_path)?;
        if path.is_dir() {
            tokio::fs::remove_dir(&path).await?;
        } else {
            tokio::fs::remove_file(&path).await?;
        }
        Ok(())
    }

    /// 创建沙箱内的目录
    pub async fn create_dir(&self, sandbox_id: &str, relative_path: &str) -> Result<(), SandboxError> {
        let path = self.resolve_safe(sandbox_id, relative_path)?;
        tokio::fs::create_dir_all(&path).await?;
        Ok(())
    }

    /// 列出沙箱内目录内容
    pub async fn list_dir(&self, sandbox_id: &str, relative_path: &str) -> Result<Vec<FileEntry>, SandboxError> {
        let path = self.resolve_safe(sandbox_id, relative_path)?;
        if !path.is_dir() {
            return Err(SandboxError::NotADirectory(path.display().to_string()));
        }

        let mut entries = Vec::new();
        let mut read_dir = tokio::fs::read_dir(&path).await?;
        while let Some(entry) = read_dir.next_entry().await? {
            let meta = entry.metadata().await?;
            let name = entry.file_name().to_string_lossy().to_string();
            // 隐藏 .votek-sandbox 内部目录
            if name == ".votek-sandbox" {
                continue;
            }
            let rel = if relative_path.is_empty() || relative_path == "." {
                name.clone()
            } else {
                format!("{}/{}", relative_path.trim_end_matches('/'), name)
            };
            entries.push(FileEntry {
                name,
                path: rel,
                is_dir: meta.is_dir(),
                size: meta.len(),
            });
        }
        // 排序：目录在前，按名称排序
        entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
        Ok(entries)
    }

    /// 递归获取沙箱内的文件树（前 200 项，超过截断）
    pub async fn file_tree(&self, sandbox_id: &str, max_entries: usize) -> Result<Vec<FileEntry>, SandboxError> {
        let sb = self.sandboxes.get(sandbox_id)
            .ok_or_else(|| SandboxError::NotFound(sandbox_id.to_string()))?;
        let mut entries = Vec::new();
        self.walk_dir(&sb.root, "", &mut entries, max_entries).await?;
        Ok(entries)
    }

    async fn walk_dir(&self, base: &Path, prefix: &str, entries: &mut Vec<FileEntry>, max: usize) -> Result<(), SandboxError> {
        if entries.len() >= max {
            return Ok(());
        }
        let mut read_dir = tokio::fs::read_dir(base).await?;
        while let Some(entry) = read_dir.next_entry().await? {
            if entries.len() >= max {
                break;
            }
            let meta = entry.metadata().await?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name == ".votek-sandbox" || name.starts_with('.') {
                continue;
            }
            let rel = if prefix.is_empty() { name.clone() } else { format!("{}/{}", prefix, name) };
            let is_dir = meta.is_dir();
            let sub_dir = base.join(&name);
            entries.push(FileEntry { name, path: rel.clone(), is_dir, size: meta.len() });
            if is_dir {
                Box::pin(self.walk_dir(&sub_dir, &rel, entries, max)).await?;
            }
        }
        Ok(())
    }
}

// ── 测试 ──

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[tokio::test]
    async fn test_sandbox_lifecycle() {
        let tmp = std::env::temp_dir().join("votek_sandbox_test_lifecycle");
        fs::create_dir_all(&tmp).unwrap();

        let mut mgr = SandboxManager::new();

        // 创建
        let info = mgr.create("test1", "测试沙箱", &tmp).unwrap();
        assert_eq!(info.id, "test1");
        assert!(tmp.join(".votek-sandbox").exists());

        // 写文件
        mgr.write_file("test1", "hello.txt", "Hello World").await.unwrap();
        let content = mgr.read_file("test1", "hello.txt").await.unwrap();
        assert_eq!(content, "Hello World");

        // 列目录
        let entries = mgr.list_dir("test1", ".").await.unwrap();
        assert!(entries.iter().any(|e| e.name == "hello.txt"));

        // 路径越界检测
        let result = mgr.read_file("test1", "../etc/passwd").await;
        assert!(result.is_err());

        // 删除
        mgr.remove("test1").unwrap();
        assert!(!tmp.join(".votek-sandbox").exists());

        fs::remove_dir_all(&tmp).ok();
    }
}
