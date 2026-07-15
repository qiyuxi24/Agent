/// build.rs — Votek 编译预处理
///
/// 仅负责检测 + 提示，不做实际下载/编译。
/// 所有 code-server 准备统一由 scripts/download-code-server.mjs 负责。
/// 原生模块清单来自 agent-desktop/build.config.json（单一真相源），通过
/// include_str! 在编译时嵌入并解析，消除与 download-code-server.mjs 的重复。
use std::path::Path;

/// 从 build.config.json 的 codeServer.nativeModules 解析出 [(pkg, file)] 列表。
/// 结构固定为 [{ "pkg": "xxx", "file": "yyy" }, ...]，零依赖手搓提取。
fn parse_native_modules(config_json: &str) -> Vec<(&'static str, &'static str)> {
    let mut modules = Vec::new();
    // 定位 nativeModules 数组段
    if let Some(start) = config_json.find("\"nativeModules\"") {
        let rest = &config_json[start..];
        // 找到最近的 [ ... ] 范围（允许嵌套 {} 但不考虑嵌套 []）
        if let Some(arr_start) = rest.find('[') {
            let depth_start = &rest[arr_start..];
            let mut depth = 0usize;
            let mut end = 0usize;
            for (i, ch) in depth_start.char_indices() {
                match ch {
                    '[' => depth += 1,
                    ']' => {
                        depth -= 1;
                        if depth == 0 {
                            end = i + 1;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let array_text = &depth_start[..end];
            // 逐一提取 "pkg": "..." 和 "file": "..." 对
            let mut idx = 0;
            while idx < array_text.len() {
                let slice = &array_text[idx..];
                if let Some(pkg_start) = slice.find("\"pkg\"") {
                    let after_pkg = &slice[pkg_start..];
                    // 提取 pkg 值
                    if let Some(pkg_val) = extract_json_string(after_pkg) {
                        // 在 pkg 之后找 file
                        let file_search = &after_pkg[after_pkg.find("\"pkg\"").unwrap()..];
                        if let Some(file_start_pos) = file_search.find("\"file\"") {
                            let after_file_marker = &file_search[file_start_pos..];
                            if let Some(file_val) = extract_json_string(after_file_marker) {
                                // leak 为 'static — build.rs 进程生命周期内安全
                                let pkg: &'static str = Box::leak(pkg_val.into_boxed_str());
                                let file: &'static str = Box::leak(file_val.into_boxed_str());
                                modules.push((pkg, file));
                            }
                        }
                    }
                    // 跳到下一个对象（找下一个 { 或到达数组末尾）
                    let next_obj = after_pkg[after_pkg.find('{').unwrap_or(0)..].find('}');
                    idx += pkg_start + next_obj.unwrap_or(after_pkg.len()) + 1;
                } else {
                    break;
                }
            }
        }
    }
    modules
}

/// 从 JSON 片段中提取紧跟在已知 key 后的字符串值 "..."
fn extract_json_string(s: &str) -> Option<String> {
    // 找 ": " 后面的 "
    let colon = s.find(": ")?; 
    let after_colon = &s[colon + 2..];
    if !after_colon.starts_with('"') { return None; }
    let end = after_colon[1..].find('"')?;
    Some(after_colon[1..=end].to_string())
}

fn main() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let release_dir = manifest_dir
        .join("binaries")
        .join("code-server")
        .join("release");
    let entry = release_dir.join("out").join("node").join("entry.js");

    // ---- code-server 就绪检查 ----
    if !entry.exists() {
        println!("cargo:warning=┌─────────────────────────────────────────────");
        println!("cargo:warning=│ code-server NOT FOUND");
        println!("cargo:warning=│ Required for the built-in IDE feature.");
        println!("cargo:warning=│");
        println!("cargo:warning=│ Run this ONE command to set up:");
        println!("cargo:warning=│   npm run download:code-server");
        println!("cargo:warning=│");
        println!("cargo:warning=│ Or from repo root:");
        println!("cargo:warning=│   node scripts/download-code-server.mjs");
        println!("cargo:warning=└─────────────────────────────────────────────");
    } else {
        // 从 build.config.json（单一真相源）读取原生模块清单
        let config = include_str!("../build.config.json");
        let modules = parse_native_modules(config);
        if modules.is_empty() {
            // 解析失败时回退到硬编码兜底 + 编译警告
            println!("cargo:warning=⚠ 无法从 build.config.json 解析原生模块清单，使用内置兜底列表");
            let fallback: [(&str, &str); 7] = [
                ("windows-registry", "winregistry.node"),
                ("windows-process-tree", "windows_process_tree.node"),
                ("deviceid", "windows.node"),
                ("native-watchdog", "watchdog.node"),
                ("spdlog", "spdlog.node"),
                ("sqlite3", "vscode-sqlite3.node"),
                ("windows-ca-certs", "crypt32.node"),
            ];
            check_native_modules_dynamic(&release_dir, &fallback);
        } else {
            check_native_modules_dynamic(&release_dir, &modules);
        }
    }

    tauri_build::build();
}

/// 检测 @vscode/* 原生 .node 模块（动态清单，与 build.config.json 对齐）
/// 缺失时仅提示，不阻止编译（IDE 功能不可用但应用可启动）
fn check_native_modules_dynamic(release_dir: &Path, modules: &[(&str, &str)]) {
    let vscode_dir = release_dir
        .join("lib")
        .join("vscode")
        .join("node_modules")
        .join("@vscode");

    let mut missing = Vec::new();
    for (pkg, file) in modules {
        let path = vscode_dir.join(pkg).join("build").join("Release").join(file);
        if !path.exists() {
            missing.push(format!("@{}/{}", pkg, file));
        }
    }

    if !missing.is_empty() {
        println!("cargo:warning=┌─────────────────────────────────────────────");
        println!(
            "cargo:warning=│ {} native module(s) missing from code-server:",
            missing.len()
        );
        for m in &missing {
            println!("cargo:warning=│   {}", m);
        }
        println!("cargo:warning=│");
        println!("cargo:warning=│ These are required for the IDE feature.");
        println!("cargo:warning=│ Run: npm run download:code-server");
        println!("cargo:warning=└─────────────────────────────────────────────");
    } else {
        println!(
            "cargo:warning=✅ All {} native modules verified OK (from build.config.json)",
            modules.len()
        );
    }
}
