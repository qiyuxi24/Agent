// ============================================================
// RAG 模块 — 检索增强生成
// 架构：fastembed 嵌入 + text-splitter 分块 + LanceDB 存储 + Tauri 命令
// 消费级产品：轻量、本地推理、首次自动下载模型、离线可用
// ============================================================

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

// Arrow / LanceDB
use arrow::record_batch::RecordBatchIterator;
use arrow_array::{
    builder::Int32Builder, Float32Array, Float64Array, FixedSizeListArray, Int32Array, RecordBatch,
    StringArray,
};
use arrow_schema::{DataType, Field, Schema};
use lancedb::query::{ExecutableQuery, QueryBase};
use futures::TryStreamExt;

// 本地嵌入 & 语义分块
use fastembed::{EmbeddingModel, TextEmbedding, TextInitOptions};
use text_splitter::TextSplitter;

// ==============================
// Embedder trait — 抽象嵌入接口
// ==============================

/// 文本嵌入器接口（可替换为远程 API 等其他实现）
#[allow(dead_code)] // dimension/model_name 供未来远程嵌入实现使用
pub trait Embedder: Send + Sync {
    /// 批量文本 → 向量（调用方负责控制批量大小）
    fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String>;
    fn dimension(&self) -> usize;
    fn model_name(&self) -> &str;
}

/// 基于 fastembed 的本地嵌入器
/// 首次使用时自动从 HuggingFace 下载 ONNX 模型（~47MB），之后离线运行
/// 内部用 Mutex 包裹 TextEmbedding（ONNX 运行时非 Sync）
pub struct FastembedEmbedder {
    model: std::sync::Mutex<TextEmbedding>,
    #[allow(dead_code)]
    dimension: usize,
    #[allow(dead_code)]
    model_name_str: String,
}

impl FastembedEmbedder {
    /// 创建嵌入器，model_variant 对应 EmbeddingModel 枚举值
    /// 默认推荐 BAAI/bge-small-zh-v1.5（中文、512维、快速）
    pub fn try_new(model_variant: EmbeddingModel) -> Result<Self, String> {
        let model_name_str = format!("{:?}", model_variant);
        eprintln!(
            "[RAG] 加载嵌入模型 '{}'，首次使用将自动下载...",
            model_name_str
        );

        let mut model = TextEmbedding::try_new(TextInitOptions::new(model_variant))
            .map_err(|e| format!("加载嵌入模型失败（需要网络下载模型）: {}", e))?;

        // 通过单次推理获取实际维度
        let dimension = model
            .embed(vec!["hello".to_string()], None)
            .map(|v| v.first().map(|vec| vec.len()).unwrap_or(0))
            .map_err(|e| format!("获取向量维度失败: {}", e))?;

        if dimension == 0 {
            return Err("模型返回了空向量".to_string());
        }

        eprintln!(
            "[RAG] 嵌入模型 '{}' 就绪，维度={}",
            model_name_str, dimension
        );

        Ok(Self {
            model: std::sync::Mutex::new(model),
            dimension,
            model_name_str,
        })
    }
}

impl Embedder for FastembedEmbedder {
    fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        let mut model = self
            .model
            .lock()
            .map_err(|e| format!("嵌入器锁错误: {}", e))?;
        // fastembed 接受 Vec<String>
        let texts_vec: Vec<String> = texts.to_vec();
        model
            .embed(texts_vec, None)
            .map_err(|e| format!("嵌入计算失败: {}", e))
    }

    fn dimension(&self) -> usize {
        self.dimension
    }

    fn model_name(&self) -> &str {
        &self.model_name_str
    }
}

// ==============================
// 配置
// ==============================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagConfig {
    /// 是否启用 RAG
    pub enabled: bool,
    /// LanceDB 数据库路径
    #[serde(rename = "dbPath")]
    pub db_path: String,
    /// 嵌入模型名称
    #[serde(rename = "embeddingModel")]
    pub embedding_model: String,
    /// 向量维度
    #[serde(rename = "embeddingDimension")]
    pub embedding_dimension: usize,
    /// 分块大小（字符数）
    #[serde(rename = "chunkSize")]
    pub chunk_size: usize,
    /// 分块重叠（字符数）
    #[serde(rename = "chunkOverlap")]
    pub chunk_overlap: usize,
    /// 检索返回条数
    #[serde(rename = "topK")]
    pub top_k: usize,
}

impl Default for RagConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            db_path: String::new(),
            embedding_model: "BAAI/bge-small-zh-v1.5".to_string(),
            embedding_dimension: 512, // bge-small-zh 的维度
            chunk_size: 512,
            chunk_overlap: 0, // 语义分块不需要重叠
            top_k: 5,
        }
    }
}

// ==============================
// 文档 & 检索类型
// ==============================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexRequest {
    pub documents: Vec<RagDocumentInput>,
}

/// 前端提交的文档（Tauri 命令参数）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagDocumentInput {
    pub id: String,
    pub content: String,
    /// conversation / file / webpage / manual
    #[serde(rename = "sourceType")]
    pub source_type: String,
    #[serde(rename = "sourceId")]
    pub source_id: String,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

/// 检索查询
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchQuery {
    pub query: String,
    #[serde(rename = "topK")]
    pub top_k: Option<usize>,
    /// 可选：按来源类型过滤
    #[serde(rename = "sourceType")]
    pub source_type_filter: Option<String>,
}

/// 检索结果项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResultItem {
    #[serde(rename = "documentId")]
    pub document_id: String,
    pub content: String,
    /// 相似度分数 (0-1，越高越相关)
    pub score: f32,
    #[serde(rename = "sourceType")]
    pub source_type: String,
    #[serde(rename = "sourceId")]
    pub source_id: String,
    #[serde(rename = "chunkIndex")]
    pub chunk_index: i32,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

/// 索引统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagStats {
    #[serde(rename = "documentCount")]
    pub document_count: usize,
    #[serde(rename = "chunkCount")]
    pub chunk_count: usize,
    pub config: RagConfig,
}

// ==============================
// 文本分块器
// ==============================

/// 语义分块：递归寻找最高语义层级边界（段落 > 句子 > 词 > 字符）
/// 不再使用滑动窗口重叠——语义边界本身就保证了上下文的连贯性
fn split_text(text: &str, chunk_size: usize, _overlap: usize) -> Vec<String> {
    let splitter = TextSplitter::new(chunk_size);
    splitter.chunks(text).map(|s| s.to_string()).collect()
}

// ==============================
// LanceDB 表 Schema
// ==============================

const TABLE_NAME: &str = "rag_documents";

fn make_schema(vector_dim: i32) -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("content", DataType::Utf8, false),
        Field::new("source_type", DataType::Utf8, false),
        Field::new("source_id", DataType::Utf8, false),
        Field::new("chunk_index", DataType::Int32, false),
        Field::new("metadata_json", DataType::Utf8, true),
        Field::new(
            "vector",
            DataType::FixedSizeList(
                Arc::new(Field::new("item", DataType::Float32, true)),
                vector_dim,
            ),
            false,
        ),
    ]))
}

/// 内部记录结构（构建 Arrow RecordBatch 用）
struct RagRecord {
    id: String,
    content: String,
    source_type: String,
    source_id: String,
    chunk_index: i32,
    metadata_json: String,
    vector: Vec<f32>,
}

/// 将 RagRecord 列表转为 Arrow RecordBatch
fn records_to_batch(records: &[RagRecord], dim: i32) -> Result<RecordBatch, String> {
    let n = records.len();
    let dim_usize = dim as usize;

    let mut ids = Vec::with_capacity(n);
    let mut contents = Vec::with_capacity(n);
    let mut source_types = Vec::with_capacity(n);
    let mut source_ids = Vec::with_capacity(n);
    let mut chunk_indices = Int32Builder::with_capacity(n);
    let mut metadata_jsons = Vec::with_capacity(n);
    let mut flat_vectors = Vec::with_capacity(n * dim_usize);

    for r in records {
        ids.push(r.id.as_str());
        contents.push(r.content.as_str());
        source_types.push(r.source_type.as_str());
        source_ids.push(r.source_id.as_str());
        chunk_indices.append_value(r.chunk_index);
        metadata_jsons.push(r.metadata_json.as_str());
        flat_vectors.extend_from_slice(&r.vector);
    }

    let schema = make_schema(dim);

    let field = Arc::new(Field::new("item", DataType::Float32, true));
    let values = Arc::new(Float32Array::from(flat_vectors));
    let vector_array = FixedSizeListArray::try_new(field, dim, values, None)
        .map_err(|e| format!("创建向量列失败: {}", e))?;

    let batch = RecordBatch::try_new(
        schema,
        vec![
            Arc::new(StringArray::from(ids)),
            Arc::new(StringArray::from(contents)),
            Arc::new(StringArray::from(source_types)),
            Arc::new(StringArray::from(source_ids)),
            Arc::new(chunk_indices.finish()),
            Arc::new(StringArray::from(metadata_jsons)),
            Arc::new(vector_array),
        ],
    )
    .map_err(|e| format!("创建 RecordBatch 失败: {}", e))?;

    Ok(batch)
}

/// 从 Arrow RecordBatch 解析检索结果
fn parse_search_batch(batch: &RecordBatch) -> Result<Vec<SearchResultItem>, String> {
    let n = batch.num_rows();

    let ids = batch
        .column(0)
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or("id 列类型错误")?;
    let contents = batch
        .column(1)
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or("content 列类型错误")?;
    let source_types = batch
        .column(2)
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or("source_type 列类型错误")?;
    let source_ids = batch
        .column(3)
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or("source_id 列类型错误")?;
    let chunk_indices = batch
        .column(4)
        .as_any()
        .downcast_ref::<Int32Array>()
        .ok_or("chunk_index 列类型错误")?;
    let metadata_jsons = batch
        .column(5)
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or("metadata_json 列类型错误")?;

    // _distance 列（LanceDB 自动附加）
    let scores: Vec<f32> = if batch.num_columns() > 6 {
        let dist_col = batch.column(6);
        dist_col
            .as_any()
            .downcast_ref::<Float32Array>()
            .map(|arr| (0..n).map(|i| arr.value(i)).collect())
            .or_else(|| {
                dist_col
                    .as_any()
                    .downcast_ref::<Float64Array>()
                    .map(|arr| (0..n).map(|i| arr.value(i) as f32).collect())
            })
            .unwrap_or_else(|| vec![0.0f32; n])
    } else {
        vec![0.0f32; n]
    };

    let mut results = Vec::with_capacity(n);
    for i in 0..n {
        let metadata: HashMap<String, String> =
            serde_json::from_str(metadata_jsons.value(i)).unwrap_or_default();

        // 将 L2 距离转为 0-1 相似度分数
        let dist = scores[i];
        let score = 1.0 / (1.0 + dist);

        results.push(SearchResultItem {
            document_id: ids.value(i).to_string(),
            content: contents.value(i).to_string(),
            score,
            source_type: source_types.value(i).to_string(),
            source_id: source_ids.value(i).to_string(),
            chunk_index: chunk_indices.value(i),
            metadata,
        });
    }

    Ok(results)
}

// ==============================
// RagManager — 核心管理器
// ==============================

pub struct RagManager {
    config: Mutex<RagConfig>,
    embedder: Mutex<Option<Box<dyn Embedder>>>,
    db: Mutex<Option<lancedb::Connection>>,
}

impl RagManager {
    pub fn new() -> Self {
        Self {
            config: Mutex::new(RagConfig::default()),
            embedder: Mutex::new(None),
            db: Mutex::new(None),
        }
    }

    /// 初始化：连接 LanceDB + 创建表 + 设置嵌入器
    pub async fn init(&self, config: RagConfig) -> Result<(), String> {
        let db_path = config.db_path.clone();
        if db_path.is_empty() {
            return Err("dbPath 不能为空".to_string());
        }

        // 确保父目录存在
        let path = std::path::Path::new(&db_path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("无法创建数据库目录 {}: {}", parent.display(), e))?;
        }

        // 连接 LanceDB
        let db = lancedb::connect(&db_path)
            .execute()
            .await
            .map_err(|e| format!("连接 LanceDB 失败: {}", e))?;

        // 确保表存在
        let dim = config.embedding_dimension as i32;
        let table_names = db
            .table_names()
            .execute()
            .await
            .map_err(|e| format!("获取表列表失败: {}", e))?;

        if !table_names.iter().any(|t| t == TABLE_NAME) {
            db.create_empty_table(TABLE_NAME, make_schema(dim))
                .execute()
                .await
                .map_err(|e| format!("创建表失败: {}", e))?;
            eprintln!(
                "[RAG] 已创建表 '{}'，向量维度={}，路径={}",
                TABLE_NAME, dim, db_path
            );
        }

        // 设置嵌入器 —— 首次调用会自动下载 ~47MB 模型文件到本地缓存
        let model_variant = match config.embedding_model.as_str() {
            "BAAI/bge-small-zh-v1.5" => EmbeddingModel::BGESmallZHV15,
            "BAAI/bge-large-zh-v1.5" => EmbeddingModel::BGELargeZHV15,
            "BAAI/bge-m3" => EmbeddingModel::BGEM3,
            "sentence-transformers/all-MiniLM-L6-v2" => EmbeddingModel::AllMiniLML6V2,
            other => {
                return Err(format!(
                    "不支持的嵌入模型: '{}'，可选: bge-small-zh-v1.5 / bge-large-zh-v1.5 / bge-m3 / all-MiniLM-L6-v2",
                    other
                ));
            }
        };
        let embedder: Box<dyn Embedder> = Box::new(FastembedEmbedder::try_new(model_variant)?);

        // 写入状态
        *self.config.lock().await = config;
        *self.embedder.lock().await = Some(embedder);
        *self.db.lock().await = Some(db);

        eprintln!("[RAG] 初始化完成");
        Ok(())
    }

    /// 判断是否已初始化
    pub async fn is_initialized(&self) -> bool {
        self.db.lock().await.is_some()
    }

    /// 索引文档
    pub async fn index_documents(&self, docs: Vec<RagDocumentInput>) -> Result<usize, String> {
        let config = self.config.lock().await.clone();
        let db_guard = self.db.lock().await;
        let db = db_guard.as_ref().ok_or("RAG 未初始化，请先调用 rag_init")?;
        let embedder_guard = self.embedder.lock().await;
        let embedder = embedder_guard
            .as_ref()
            .ok_or("嵌入器未初始化")?;

        let table = db
            .open_table(TABLE_NAME)
            .execute()
            .await
            .map_err(|e| format!("打开表失败: {}", e))?;

        let mut records = Vec::new();

        for doc in &docs {
            let chunks = split_text(&doc.content, config.chunk_size, config.chunk_overlap);
            let texts: Vec<String> = chunks.iter().cloned().collect();
            let vectors = embedder.embed_batch(&texts)?;

            for (i, (chunk, vector)) in
                chunks.into_iter().zip(vectors.into_iter()).enumerate()
            {
                records.push(RagRecord {
                    id: format!("{}_chunk_{}", doc.id, i),
                    content: chunk,
                    source_type: doc.source_type.clone(),
                    source_id: doc.source_id.clone(),
                    chunk_index: i as i32,
                    metadata_json: serde_json::to_string(&doc.metadata).unwrap_or_default(),
                    vector,
                });
            }
        }

        let total = records.len();
        if !records.is_empty() {
            let batch = records_to_batch(&records, config.embedding_dimension as i32)?;
            let schema = batch.schema();
            let batches = RecordBatchIterator::new(
                vec![Ok(batch)].into_iter(),
                schema,
            );
            table
                .add(batches)
                .execute()
                .await
                .map_err(|e| format!("写入数据失败: {}", e))?;
            eprintln!("[RAG] 已索引 {} 条块", total);
        }

        Ok(total)
    }

    /// 删除指定文档的所有分块
    pub async fn delete_document(&self, doc_id: &str) -> Result<usize, String> {
        let db_guard = self.db.lock().await;
        let db = db_guard.as_ref().ok_or("RAG 未初始化")?;

        let table = db
            .open_table(TABLE_NAME)
            .execute()
            .await
            .map_err(|e| format!("打开表失败: {}", e))?;

        // 转义 doc_id 中的特殊字符，防止 LIKE 注入
        let escaped = doc_id
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
            .replace('\'', "''");
        table
            .delete(&format!("id LIKE '{}_chunk_%'", escaped))
            .await
            .map_err(|e| format!("删除失败: {}", e))?;

        eprintln!("[RAG] 已删除文档 '{}'", doc_id);
        Ok(0) // LanceDB 0.16 delete 不返回行数
    }

    /// 检索相关文档
    pub async fn search(&self, query: &SearchQuery) -> Result<Vec<SearchResultItem>, String> {
        let config = self.config.lock().await.clone();
        let db_guard = self.db.lock().await;
        let db = db_guard.as_ref().ok_or("RAG 未初始化")?;
        let embedder_guard = self.embedder.lock().await;
        let embedder = embedder_guard.as_ref().ok_or("嵌入器未初始化")?;

        let table = db
            .open_table(TABLE_NAME)
            .execute()
            .await
            .map_err(|e| format!("打开表失败: {}", e))?;

        // 嵌入查询
        let query_vec = embedder.embed_batch(&[query.query.clone()])?;
        let query_vec = query_vec
            .into_iter()
            .next()
            .ok_or("嵌入生成失败")?;

        let top_k = query.top_k.unwrap_or(config.top_k);

        // 向量检索
        let stream = table
            .query()
            .nearest_to(query_vec.as_slice())
            .map_err(|e| format!("创建查询失败: {}", e))?
            .limit(top_k)
            .execute()
            .await
            .map_err(|e| format!("检索失败: {}", e))?;

        let batches: Vec<RecordBatch> = stream
            .try_collect()
            .await
            .map_err(|e| format!("收集检索结果失败: {}", e))?;

        let mut items = if let Some(batch) = batches.first() {
            parse_search_batch(batch)?
        } else {
            vec![]
        };

        // 来源类型过滤
        if let Some(ref filter) = query.source_type_filter {
            items.retain(|r| r.source_type == *filter);
        }

        eprintln!("[RAG] 检索 '{}' → {} 条结果", query.query, items.len());
        Ok(items)
    }

    /// 获取索引统计
    pub async fn get_stats(&self) -> Result<RagStats, String> {
        let config = self.config.lock().await.clone();
        let db_guard = self.db.lock().await;

        if db_guard.is_none() {
            return Ok(RagStats {
                document_count: 0,
                chunk_count: 0,
                config,
            });
        }

        let db = db_guard.as_ref().unwrap();
        let table = match db.open_table(TABLE_NAME).execute().await {
            Ok(t) => t,
            Err(_) => {
                return Ok(RagStats {
                    document_count: 0,
                    chunk_count: 0,
                    config,
                })
            }
        };

        let chunk_count = table
            .count_rows(None)
            .await
            .map_err(|e| format!("统计行数失败: {}", e))?;

        Ok(RagStats {
            document_count: 0, // TODO: 统计去重后的文档数
            chunk_count,
            config,
        })
    }

    /// 清空所有索引
    pub async fn clear_all(&self) -> Result<(), String> {
        let db_guard = self.db.lock().await;
        let db = db_guard.as_ref().ok_or("RAG 未初始化")?;

        let table = db
            .open_table(TABLE_NAME)
            .execute()
            .await
            .map_err(|e| format!("打开表失败: {}", e))?;

        table
            .delete("true")
            .await
            .map_err(|e| format!("清空表失败: {}", e))?;

        eprintln!("[RAG] 已清空索引");
        Ok(())
    }
}

// ==============================
// Tauri Commands
// ==============================

/// 初始化 RAG 系统
#[tauri::command]
pub async fn rag_init(
    state: tauri::State<'_, crate::AppState>,
    config: RagConfig,
) -> Result<(), String> {
    state.rag.init(config).await
}

/// 获取 RAG 配置
#[tauri::command]
pub async fn rag_get_config(
    state: tauri::State<'_, crate::AppState>,
) -> Result<RagConfig, String> {
    Ok(state.rag.config.lock().await.clone())
}

/// 获取索引统计
#[tauri::command]
pub async fn rag_get_stats(
    state: tauri::State<'_, crate::AppState>,
) -> Result<RagStats, String> {
    state.rag.get_stats().await
}

/// 索引文档
#[tauri::command]
pub async fn rag_index_documents(
    state: tauri::State<'_, crate::AppState>,
    request: IndexRequest,
) -> Result<usize, String> {
    state.rag.index_documents(request.documents).await
}

/// 检索
#[tauri::command]
pub async fn rag_search(
    state: tauri::State<'_, crate::AppState>,
    query: SearchQuery,
) -> Result<Vec<SearchResultItem>, String> {
    state.rag.search(&query).await
}

/// 删除文档
#[tauri::command]
pub async fn rag_delete_document(
    state: tauri::State<'_, crate::AppState>,
    #[allow(unused_variables)] doc_id: String,
) -> Result<usize, String> {
    state.rag.delete_document(&doc_id).await
}

/// 清空所有索引
#[tauri::command]
pub async fn rag_clear_all(
    state: tauri::State<'_, crate::AppState>,
) -> Result<(), String> {
    state.rag.clear_all().await
}

/// 为对话检索上下文（Agent 工具调用入口）
#[tauri::command]
pub async fn rag_search_for_chat(
    state: tauri::State<'_, crate::AppState>,
    query: String,
    top_k: Option<usize>,
) -> Result<String, String> {
    let search = SearchQuery {
        query,
        top_k,
        source_type_filter: None,
    };
    let results = state.rag.search(&search).await?;

    if results.is_empty() {
        return Ok("未找到相关知识。".to_string());
    }

    let context = results
        .iter()
        .enumerate()
        .map(|(i, r)| {
            format!(
                "[来源 {}] (score: {:.2}, source: {}/{})\n{}",
                i + 1,
                r.score,
                r.source_type,
                r.source_id,
                r.content
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n---\n\n");

    Ok(context)
}
