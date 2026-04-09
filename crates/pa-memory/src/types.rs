// MAGMA 多图谱智能体记忆架构 - 核心类型定义
// 本模块定义了 MAGMA 架构中所有核心数据结构，包括记忆节点、图边、图谱类型、查询意图等。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 记忆节点类型 - 描述记忆的来源和性质
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MemoryNodeType {
    /// 观察类记忆 - 来自外部感知的信息
    Observation,
    /// 动作类记忆 - 智能体执行的操作
    Action,
    /// 状态变化 - 环境或智能体状态的改变
    StateChange,
    /// 推断类记忆 - 通过推理得出的结论
    Inferred,
}

impl std::fmt::Display for MemoryNodeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MemoryNodeType::Observation => write!(f, "观察"),
            MemoryNodeType::Action => write!(f, "动作"),
            MemoryNodeType::StateChange => write!(f, "状态变化"),
            MemoryNodeType::Inferred => write!(f, "推断"),
        }
    }
}

/// 事件节点 - MAGMA 的基本记忆单元
/// 每个节点代表一个事件或知识片段，可同时存在于多个正交图谱中
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryNode {
    /// 节点唯一标识符
    pub id: String,
    /// 事件内容描述
    pub content: String,
    /// 事件发生时间戳
    pub timestamp: DateTime<Utc>,
    /// 密集向量表示（用于语义检索）
    pub embedding: Option<Vec<f64>>,
    /// 结构化元数据（灵活的键值对存储）
    pub attributes: HashMap<String, serde_json::Value>,
    /// 节点类型
    pub node_type: MemoryNodeType,
}

impl MemoryNode {
    /// 创建新的记忆节点
    pub fn new(content: impl Into<String>, node_type: MemoryNodeType) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            content: content.into(),
            timestamp: Utc::now(),
            embedding: None,
            attributes: HashMap::new(),
            node_type,
        }
    }

    /// 创建带有指定 ID 的记忆节点（用于测试和恢复）
    pub fn with_id(id: impl Into<String>, content: impl Into<String>, node_type: MemoryNodeType) -> Self {
        Self {
            id: id.into(),
            content: content.into(),
            timestamp: Utc::now(),
            embedding: None,
            attributes: HashMap::new(),
            node_type,
        }
    }

    /// 设置节点嵌入向量
    pub fn with_embedding(mut self, embedding: Vec<f64>) -> Self {
        self.embedding = Some(embedding);
        self
    }

    /// 添加元数据属性
    pub fn with_attribute(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.attributes.insert(key.into(), value);
        self
    }

    /// 获取访问频率（从元数据中读取）
    pub fn access_count(&self) -> usize {
        self.attributes
            .get("access_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize
    }

    /// 增加访问频率
    pub fn increment_access(&mut self) {
        let count = self.access_count() + 1;
        self.attributes
            .insert("access_count".to_string(), serde_json::Value::Number(count.into()));
    }
}

/// 图边类型 - 定义节点间关系的语义
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EdgeType {
    /// 语义相似 - 基于内容相似度的关联
    SemanticSimilarity,
    /// 时间顺序 - source 在 target 之前发生
    TemporalBefore,
    /// 因果关系 - source 导致了 target
    Causal,
    /// 实体关系 - 自定义实体间关系
    EntityRelation {
        /// 关系名称（如 "属于"、"位于" 等）
        relation: String,
    },
    /// 层次关系 - source 是 target 的父级/抽象
    Hierarchical,
}

impl std::fmt::Display for EdgeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EdgeType::SemanticSimilarity => write!(f, "语义相似"),
            EdgeType::TemporalBefore => write!(f, "时间顺序"),
            EdgeType::Causal => write!(f, "因果关系"),
            EdgeType::EntityRelation { relation } => write!(f, "实体关系:{}", relation),
            EdgeType::Hierarchical => write!(f, "层次关系"),
        }
    }
}

/// 图边 - 连接两个记忆节点的关系
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    /// 源节点 ID
    pub source_id: String,
    /// 目标节点 ID
    pub target_id: String,
    /// 边类型
    pub edge_type: EdgeType,
    /// 关系权重（0.0 ~ 1.0，值越大表示关系越强）
    pub weight: f64,
    /// 边的元数据
    pub metadata: HashMap<String, serde_json::Value>,
}

impl GraphEdge {
    /// 创建新的图边
    pub fn new(
        source_id: impl Into<String>,
        target_id: impl Into<String>,
        edge_type: EdgeType,
        weight: f64,
    ) -> Self {
        Self {
            source_id: source_id.into(),
            target_id: target_id.into(),
            edge_type,
            weight: weight.clamp(0.0, 1.0),
            metadata: HashMap::new(),
        }
    }

    /// 创建带有元数据的图边
    pub fn with_metadata(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }
}

/// 四个正交图谱类型
/// MAGMA 使用四个独立的图谱来管理不同维度的记忆关系
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GraphType {
    /// 语义图 - 管理主题间概念关联，基于内容相似度
    Semantic,
    /// 时间图 - 记录事件时间顺序和因果链
    Temporal,
    /// 因果图 - 明确建模因果和影响关系
    Causal,
    /// 实体图 - 跟踪现实世界实体间关系
    Entity,
}

impl std::fmt::Display for GraphType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GraphType::Semantic => write!(f, "语义图"),
            GraphType::Temporal => write!(f, "时间图"),
            GraphType::Causal => write!(f, "因果图"),
            GraphType::Entity => write!(f, "实体图"),
        }
    }
}

impl GraphType {
    /// 获取该图谱类型对应的默认边类型
    pub fn default_edge_type(&self) -> EdgeType {
        match self {
            GraphType::Semantic => EdgeType::SemanticSimilarity,
            GraphType::Temporal => EdgeType::TemporalBefore,
            GraphType::Causal => EdgeType::Causal,
            GraphType::Entity => EdgeType::EntityRelation {
                relation: "related".to_string(),
            },
        }
    }

    /// 获取所有图谱类型
    pub fn all() -> &'static [GraphType] {
        &[
            GraphType::Semantic,
            GraphType::Temporal,
            GraphType::Causal,
            GraphType::Entity,
        ]
    }
}

/// 查询意图 - 描述用户的查询目的，用于自适应检索策略选择
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum QueryIntent {
    /// 事实检查 - 优先查询实体图和语义图
    Factual,
    /// 时间线跟踪 - 专注时间图
    Temporal,
    /// 根本原因分析 - 从因果图开始多跳推理
    Causal,
    /// 开放域 - 全图谱搜索
    OpenDomain,
}

impl std::fmt::Display for QueryIntent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QueryIntent::Factual => write!(f, "事实检查"),
            QueryIntent::Temporal => write!(f, "时间线跟踪"),
            QueryIntent::Causal => write!(f, "根本原因分析"),
            QueryIntent::OpenDomain => write!(f, "开放域"),
        }
    }
}

impl QueryIntent {
    /// 根据查询意图获取优先查询的图谱列表
    pub fn priority_graphs(&self) -> Vec<GraphType> {
        match self {
            QueryIntent::Factual => vec![GraphType::Entity, GraphType::Semantic],
            QueryIntent::Temporal => vec![GraphType::Temporal],
            QueryIntent::Causal => vec![GraphType::Causal, GraphType::Temporal],
            QueryIntent::OpenDomain => GraphType::all().to_vec(),
        }
    }

    /// 根据查询意图获取最大遍历跳数
    pub fn max_hops(&self) -> usize {
        match self {
            QueryIntent::Factual => 2,
            QueryIntent::Temporal => 5,
            QueryIntent::Causal => 4,
            QueryIntent::OpenDomain => 3,
        }
    }
}

/// 遍历路径 - 图谱遍历过程中发现的路径
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraversalPath {
    /// 路径上的节点 ID 序列
    pub nodes: Vec<String>,
    /// 路径上的边类型序列
    pub edge_types: Vec<EdgeType>,
    /// 路径评分（综合权重）
    pub score: f64,
}

impl TraversalPath {
    /// 创建新的遍历路径
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            edge_types: Vec::new(),
            score: 0.0,
        }
    }

    /// 添加一个节点和对应的边到路径中
    pub fn push(&mut self, node_id: String, edge_type: EdgeType) {
        self.nodes.push(node_id);
        self.edge_types.push(edge_type);
    }

    /// 计算路径的平均权重
    pub fn average_weight(&self) -> f64 {
        if self.edge_types.is_empty() {
            return 0.0;
        }
        // score 本身已经代表了综合权重
        self.score
    }

    /// 路径长度（跳数）
    pub fn hop_count(&self) -> usize {
        self.nodes.len().saturating_sub(1)
    }
}

/// 检索结果 - 查询引擎返回的综合结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalResult {
    /// 检索到的相关记忆节点
    pub nodes: Vec<MemoryNode>,
    /// 相关遍历路径
    pub paths: Vec<TraversalPath>,
    /// 检索置信度（0.0 ~ 1.0）
    pub confidence: f64,
    /// 检索时使用的查询意图
    pub intent: QueryIntent,
}

impl RetrievalResult {
    /// 创建空的检索结果
    pub fn empty(intent: QueryIntent) -> Self {
        Self {
            nodes: Vec::new(),
            paths: Vec::new(),
            confidence: 0.0,
            intent,
        }
    }

    /// 合并多个检索结果
    pub fn merge(results: Vec<RetrievalResult>) -> Self {
        if results.is_empty() {
            return Self::empty(QueryIntent::OpenDomain);
        }

        let intent = results[0].intent.clone();
        let results_len = results.len();
        let mut all_nodes: Vec<MemoryNode> = Vec::new();
        let mut all_paths: Vec<TraversalPath> = Vec::new();
        let mut total_confidence = 0.0;

        for result in results {
            all_nodes.extend(result.nodes);
            all_paths.extend(result.paths);
            total_confidence += result.confidence;
        }

        // 去重节点
        all_nodes.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        all_nodes.dedup_by(|a, b| a.id == b.id);

        // 按评分排序路径
        all_paths.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        Self {
            nodes: all_nodes,
            paths: all_paths,
            confidence: total_confidence / results_len as f64,
            intent,
        }
    }
}

/// 查询分析结果 - 查询引擎第一阶段的分析输出
#[derive(Debug, Clone)]
pub struct QueryAnalysis {
    /// 原始查询文本
    pub raw_query: String,
    /// 提取的关键词
    pub keywords: Vec<String>,
    /// 推断的查询意图
    pub intent: QueryIntent,
    /// 查询中的时间引用
    pub temporal_references: Vec<String>,
    /// 查询中的实体引用
    pub entity_references: Vec<String>,
}

/// 整合报告 - 慢速整合流程的执行结果
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IntegrationReport {
    /// 合并的节点数
    pub nodes_merged: usize,
    /// 推断的新边数
    pub edges_inferred: usize,
    /// 解决的矛盾数
    pub contradictions_resolved: usize,
    /// 修剪的节点数
    pub nodes_pruned: usize,
}

impl IntegrationReport {
    /// 检查是否有任何整合操作执行
    pub fn has_changes(&self) -> bool {
        self.nodes_merged > 0
            || self.edges_inferred > 0
            || self.contradictions_resolved > 0
            || self.nodes_pruned > 0
    }
}

/// 矛盾解决动作
#[derive(Debug, Clone)]
pub enum ResolutionAction {
    /// 合并两个节点，保留较新的内容
    Merge {
        /// 保留的节点 ID
        keep_id: String,
        /// 被移除的节点 ID
        remove_id: String,
    },
    /// 标记节点为过时
    MarkOutdated {
        /// 被标记的节点 ID
        node_id: String,
        /// 过时原因
        reason: String,
    },
    /// 保留两者，添加矛盾标记
    KeepBoth {
        /// 相矛盾的节点 ID 对
        node_ids: (String, String),
    },
}

/// 记忆配置 - MAGMA 引擎的配置参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    /// 初始向量搜索结果数（默认 20）
    pub vector_search_k: usize,
    /// 最小关键词匹配分数（默认 0.3）
    pub keyword_threshold: f64,
    /// 最终上下文中的节点数（默认 5）
    pub top_k_final: usize,
    /// 图谱遍历最大跳数（默认 3）
    pub max_traversal_hops: usize,
    /// 语义相似度阈值（默认 0.7）
    pub similarity_threshold: f64,
    /// 是否启用慢速整合（默认 true）
    pub enable_slow_integration: bool,
    /// 重复检测相似度阈值（默认 0.85）
    pub duplicate_threshold: f64,
    /// 低频节点修剪阈值（默认 2，访问次数低于此值的节点将被修剪）
    pub prune_frequency_threshold: usize,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            vector_search_k: 20,
            keyword_threshold: 0.3,
            top_k_final: 5,
            max_traversal_hops: 3,
            similarity_threshold: 0.7,
            enable_slow_integration: true,
            duplicate_threshold: 0.85,
            prune_frequency_threshold: 2,
        }
    }
}

/// MAGMA 记忆引擎错误类型
#[derive(Debug, thiserror::Error)]
pub enum MemoryError {
    /// 节点未找到
    #[error("节点未找到: {0}")]
    NodeNotFound(String),

    /// 图操作错误
    #[error("图操作失败: {0}")]
    GraphError(String),

    /// 向量操作错误
    #[error("向量操作失败: {0}")]
    VectorError(String),

    /// 查询错误
    #[error("查询失败: {0}")]
    QueryError(String),

    /// 整合错误
    #[error("整合失败: {0}")]
    IntegrationError(String),

    /// 配置错误
    #[error("配置错误: {0}")]
    ConfigError(String),
}

/// 记忆操作的通用结果类型
pub type Result<T> = std::result::Result<T, MemoryError>;
