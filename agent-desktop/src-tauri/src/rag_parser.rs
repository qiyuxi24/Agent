// ============================================================
// rag_parser.rs — 文档格式解析器
// 支持：TXT / MD / PDF / DOCX
// 纯 Rust 实现，零系统级依赖
// ============================================================

use std::io::Read;

/// 检测到的文件类型
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FileType {
    Txt,
    Md,
    Pdf,
    Docx,
    Unknown,
}

impl FileType {
    /// 根据文件扩展名判断类型
    pub fn from_ext(path: &str) -> Self {
        let lower = path.to_lowercase();
        if lower.ends_with(".txt") {
            Self::Txt
        } else if lower.ends_with(".md") || lower.ends_with(".markdown") {
            Self::Md
        } else if lower.ends_with(".pdf") {
            Self::Pdf
        } else if lower.ends_with(".docx") {
            Self::Docx
        } else {
            Self::Unknown
        }
    }
}

/// 解析结果
#[derive(Debug, Clone)]
pub struct ParseResult {
    /// 提取的文本内容
    pub text: String,
    /// 检测到的文件类型
    pub file_type: FileType,
}

/// 从文件字节中提取文本
///
/// # 参数
/// * `bytes` - 文件原始字节
/// * `path` - 文件路径（仅用于扩展名检测）
///
/// # 返回
/// 提取的文本 + 文件类型，或错误信息
pub fn extract_text(bytes: &[u8], path: &str) -> Result<ParseResult, String> {
    let file_type = FileType::from_ext(path);

    match file_type {
        FileType::Txt | FileType::Md => {
            let text = String::from_utf8(bytes.to_vec())
                .map_err(|e| format!("文件编码不是有效的 UTF-8: {}", e))?;
            Ok(ParseResult { text, file_type })
        }
        FileType::Pdf => {
            let text = pdf_extract::extract_text_from_mem(bytes)
                .map_err(|e| format!("PDF 解析失败: {}", e))?;
            // 清理空行过多的文本
            let text = clean_text(&text);
            Ok(ParseResult { text, file_type })
        }
        FileType::Docx => {
            let text = parse_docx(bytes)?;
            let text = clean_text(&text);
            Ok(ParseResult { text, file_type })
        }
        FileType::Unknown => Err(format!(
            "不支持的文件格式。支持的格式：.txt, .md, .pdf, .docx"
        )),
    }
}

/// 解析 DOCX 文件（DOCX 本质是 ZIP 包，内含 XML）
fn parse_docx(bytes: &[u8]) -> Result<String, String> {
    use zip::ZipArchive;

    let reader = std::io::Cursor::new(bytes);
    let mut archive =
        ZipArchive::new(reader).map_err(|e| format!("无法打开 DOCX 文件: {}", e))?;

    // DOCX 文档内容在 word/document.xml 中
    let mut found = false;
    let mut text = String::new();

    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| format!("读取 DOCX 条目失败: {}", e))?;
        let name = file.name().to_lowercase();

        if name == "word/document.xml" {
            found = true;
            let mut xml_content = String::new();
            file.read_to_string(&mut xml_content)
                .map_err(|e| format!("读取 DOCX XML 失败: {}", e))?;
            text = extract_text_from_docx_xml(&xml_content);
            break;
        }
    }

    if !found {
        return Err("DOCX 文件中未找到文档内容（缺少 word/document.xml）".to_string());
    }

    Ok(text)
}

/// 从 DOCX 的 word/document.xml 中提取纯文本
///
/// 解析 <w:t> 标签（Word 文本标签）的内容，忽略所有格式标记。
/// 在 <w:p>（段落）末尾添加换行。
fn extract_text_from_docx_xml(xml: &str) -> String {
    let mut result = String::new();
    let mut in_t_tag = false;
    let mut tag_name = String::new();

    let chars: Vec<char> = xml.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '<' {
            tag_name.clear();
            i += 1;

            let is_end = i < chars.len() && chars[i] == '/';

            // 跳过注释
            if i + 2 < chars.len() && chars[i] == '!' && chars[i + 1] == '-' && chars[i + 2] == '-' {
                while i < chars.len() {
                    if chars[i] == '>' && i >= 2 && chars[i - 1] == '-' && chars[i - 2] == '-' {
                        break;
                    }
                    i += 1;
                }
                i += 1;
                continue;
            }

            // 读取到 > 或空白
            if is_end {
                i += 1; // 跳过 /
            }
            while i < chars.len() && chars[i] != '>' && !chars[i].is_whitespace() {
                tag_name.push(chars[i]);
                i += 1;
            }
            // 跳到 > 结束
            while i < chars.len() && chars[i] != '>' {
                i += 1;
            }
            i += 1; // 跳过 >

            let lower = tag_name.to_lowercase();
            if is_end {
                if lower == "w:t" {
                    in_t_tag = false;
                } else if lower == "w:p" {
                    result.push('\n');
                }
            } else {
                if lower == "w:t" {
                    in_t_tag = true;
                }
            }
        } else if in_t_tag {
            // 收集 w:t 标签内的文本
            while i < chars.len() && chars[i] != '<' {
                result.push(chars[i]);
                i += 1;
            }
        } else {
            i += 1;
        }
    }

    // 清理：移除多余空行和首尾空白
    let lines: Vec<&str> = result
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect();
    lines.join("\n")
}

/// 清理提取后的文本：压缩过多空白行、修剪首尾
fn clean_text(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut blank_count = 0u32;

    for line in text.lines() {
        if line.trim().is_empty() {
            blank_count += 1;
            if blank_count <= 2 {
                result.push('\n');
            }
        } else {
            blank_count = 0;
            result.push_str(line.trim());
            result.push('\n');
        }
    }

    result.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_type_detection() {
        assert_eq!(FileType::from_ext("doc.txt"), FileType::Txt);
        assert_eq!(FileType::from_ext("README.md"), FileType::Md);
        assert_eq!(FileType::from_ext("doc.pdf"), FileType::Pdf);
        assert_eq!(FileType::from_ext("report.docx"), FileType::Docx);
        assert_eq!(FileType::from_ext("image.png"), FileType::Unknown);
    }

    #[test]
    fn test_extract_txt() {
        let result = extract_text(b"Hello World", "test.txt").unwrap();
        assert_eq!(result.text, "Hello World");
        assert_eq!(result.file_type, FileType::Txt);
    }

    #[test]
    fn test_extract_md() {
        let content = "# Title\n\nSome **bold** text.";
        let result = extract_text(content.as_bytes(), "readme.md").unwrap();
        assert!(result.text.contains("Title"));
        assert_eq!(result.file_type, FileType::Md);
    }

    #[test]
    fn test_unsupported_format() {
        let result = extract_text(b"fake", "image.png");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("不支持"));
    }

    #[test]
    fn test_clean_text_compresses_blank_lines() {
        let input = "Line1\n\n\n\n\nLine2\n\n\nLine3";
        let cleaned = clean_text(input);
        // clean_text 保留最多 2 个连续空行作为段落分隔
        assert_eq!(cleaned, "Line1\n\n\nLine2\n\n\nLine3");
    }
}
