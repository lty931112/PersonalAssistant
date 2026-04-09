// MAGMA 多图谱智能体记忆架构 - 记忆引擎主入口
// 本模块实现了 MAGMA 记忆引擎 MagmaMemoryEngine，是整个记忆系统的主入口。
// 支持快速流（立即摄取）和慢速流（异步整合）的双流记忆管理。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::graph::InMemoryGraphDB;
use crate::integration::MemoryIntegrator;
use crate::query::{MemoryQueryEngine, QueryConfig};
use crate::types::*;
use crate::vector::InMemoryVectorStore;

/// MAGMA 记忆引擎 - 多图谱智能体记忆架构的核心
///
/// 提供记忆的摄取、整合和检索功能：
/// - 快速流（ingest_fast）：立即将新记忆添加到短期存储
/// - 慢速流（integrate_slow）：异步整合短期记忆到长期图谱
/// - 检索（retrieve）：基于查询意图的自适应记忆检索
pub struct MagmaMemoryEngine {
    /// 图数据库
    graph_db: InMemoryGraphDB,
    /// 向量存储
    vector_store: InMemoryVectorStore,
    /// 记忆配置
    config: MemoryConfig,
    /// 待整合的短期记忆队列
    pending_nodes: Vec<MemoryNode>,
}

impl MagmaMemoryEngine {
    /// 创建新的 MAGMA 记忆引擎
    ///
    /// # 参数
    /// - `config`: 记忆配置参数
    ///
    /// # 返回
    /// 初始化完成的记忆引擎实例
    pub fn new(config: &MemoryConfig) -> Result<Self> {
        info!("初始化 MAGMA 记忆引擎");

        let graph_db = InMemoryGraphDB::new();
        let vector_store = InMemoryVectorStore::new();

        Ok(Self {
            graph_db,
            vector_store,
            config: config.clone(),
            pending_nodes: Vec::new(),
        })
    }

    /// 快速流 - 立即摄取新记忆到短期存储
    ///
    /// 新记忆会被：
    /// 1. 创建为 MemoryNode 并分配唯一 ID
    /// 2. 添加到语义图（用于语义检索）
    /// 3. 添加到时间图（维护时间顺序）
    /// 4. 添加到向量存储（用于相似度搜索）
    /// 5. 加入待整合队列（等待慢速流处理）
    ///
    /// # 参数
    /// - `content`: 记忆内容
    /// - `node_type`: 记忆节点类型
    ///
    /// # 返回
    /// 新创建的记忆节点 ID
    pub async fn ingest_fast(
        &mut self,
        content: &str,
        node_type: MemoryNodeType,
    ) -> Result<String> {
        info!("快速流摄取: type={}, content='{}'", node_type, content);

        // 创建记忆节点
        let mut node = MemoryNode::new(content, node_type.clone());
        let id = node.id.clone();

        // 生成伪嵌入向量（实际系统中应调用嵌入模型）
        let embedding = self.generate_embedding(content);
        node.embedding = Some(embedding.clone());

        // 添加内容到元数据
        node.attributes.insert(
            "content".to_string(),
            serde_json::Value::String(content.to_string()),
        );
        node.attributes.insert(
            "integration_status".to_string(),
            serde_json::Value::String("pending".to_string()),
        );

        // 添加到语义图
        self.graph_db.add_node(GraphType::Semantic, node.clone());

        // 添加到时间图
        self.graph_db.add_node(GraphType::Temporal, node.clone());

        // 添加到向量存储
        let mut metadata = HashMap::new();
        metadata.insert(
            "content".to_string(),
            serde_json::Value::String(content.to_string()),
        );
        metadata.insert(
            "node_type".to_string(),
            serde_json::Value::String(node_type.to_string()),
        );
        self.vector_store.add(id.clone(), embedding, metadata);

        // 加入待整合队列
        self.pending_nodes.push(node);

        // 如果有待整合的节点，尝试建立时间边
        self.try_link_temporal(&id).await;

        debug!("快速流摄取完成，节点 ID: {}", id);
        Ok(id)
    }

    /// 慢速流 - 异步整合短期记忆到长期图谱
    ///
    /// 执行四阶段整合流程：
    /// 1. 定位（Locate）：找到需要整合的候选节点
    /// 2. 收集（Collect）：收集相关的上下文信息
    /// 3. 整合（Integrate）：合并重复、解决矛盾、推断新关系
    /// 4. 修剪和索引（Prune & Index）：清理低价值节点，更新索引
    ///
    /// # 返回
    /// 整合报告，包含操作统计
    pub async fn integrate_slow(&mut self) -> Result<IntegrationReport> {
        if !self.config.enable_slow_integration {
            debug!("慢速整合已禁用");
            return Ok(IntegrationReport::default());
        }

        if self.pending_nodes.is_empty() {
            debug!("没有待整合的节点");
            return Ok(IntegrationReport::default());
        }

        info!("开始慢速整合，待处理节点数: {}", self.pending_nodes.len());

        // 创建整合器并执行整合
        let integrator = MemoryIntegrator::new(
            &self.graph_db,
            &self.vector_store,
            &self.config,
        );

        let report = integrator.integrate().await?;

        // 将整合后的节点标记为已完成
        for node in &self.pending_nodes {
            if let Some(n) = self.graph_db.get_node_mut(&node.id) {
                n.attributes.insert(
                    "integration_status".to_string(),
                    serde_json::Value::String("integrated".to_string()),
                );
            }
        }

        // 清空待整合队列
        let integrated_count = self.pending_nodes.len();
        self.pending_nodes.clear();

        info!(
            "慢速整合完成: 合并={}, 推断={}, 矛盾={}, 修剪={}, 已处理={}",
            report.nodes_merged,
            report.edges_inferred,
            report.contradictions_resolved,
            report.nodes_pruned,
            integrated_count
        );

        Ok(report)
    }

    /// 检索记忆 - 基于查询的自适应记忆检索
    ///
    /// # 参数
    /// - `query`: 查询文本
    /// - `intent`: 可选的查询意图（不指定则自动推断）
    ///
    /// # 返回
    /// 检索结果，包含相关节点、路径和置信度
    pub async fn retrieve(
        &mut self,
        query: &str,
        intent: Option<QueryIntent>,
    ) -> Result<RetrievalResult> {
        info!("检索记忆: query='{}', intent={:?}", query, intent);

        // 创建临时查询引擎用于检索
        let query_config = QueryConfig {
            vector_search_k: self.config.vector_search_k,
            keyword_threshold: self.config.keyword_threshold,
            top_k_final: self.config.top_k_final,
            max_traversal_hops: self.config.max_traversal_hops,
            similarity_threshold: self.config.similarity_threshold,
        };

        let query_engine = MemoryQueryEngine::new(
            self.graph_db.clone(),
            self.vector_store.clone(),
            query_config,
        );

        let result = query_engine.retrieve(query, intent).await?;

        // 更新被检索节点的访问计数
        for node in &result.nodes {
            if let Some(n) = self.graph_db.get_node_mut(&node.id) {
                n.increment_access();
            }
        }

        debug!(
            "检索完成: {} 个节点, 置信度={:.3}",
            result.nodes.len(),
            result.confidence
        );

        Ok(result)
    }

    /// 添加实体关系
    ///
    /// 在实体图中创建两个实体之间的关系边
    ///
    /// # 参数
    /// - `entity_a`: 实体 A 的名称或 ID
    /// - `entity_b`: 实体 B 的名称或 ID
    /// - `relation`: 关系描述（如 "属于"、"位于" 等）
    pub async fn add_entity_relation(&mut self, entity_a: &str, entity_b: &str, relation: &str) {
        info!("添加实体关系: {} -[{}]-> {}", entity_a, relation, entity_b);

        // 查找或创建实体节点
        let id_a = self.ensure_entity_node(entity_a).await;
        let id_b = self.ensure_entity_node(entity_b).await;

        // 创建实体关系边
        let edge = GraphEdge::new(
            &id_a,
            &id_b,
            EdgeType::EntityRelation {
                relation: relation.to_string(),
            },
            0.8,
        );

        self.graph_db.add_edge(GraphType::Entity, edge);
    }

    /// 添加因果链
    ///
    /// 在因果图中创建原因节点到结果节点之间的因果关系边
    ///
    /// # 参数
    /// - `cause_id`: 原因节点 ID
    /// - `effect_id`: 结果节点 ID
    /// - `confidence`: 因果关系置信度（0.0 ~ 1.0）
    pub async fn add_causal_link(&mut self, cause_id: &str, effect_id: &str, confidence: f64) {
        info!(
            "添加因果链: {} -> {} (置信度: {:.2})",
            cause_id, effect_id, confidence
        );

        // 确保两个节点都存在于因果图中
        if let Some(node) = self.graph_db.get_node(cause_id) {
            self.graph_db.add_node(GraphType::Causal, node.clone());
        } else {
            warn!("原因节点 {} 不存在，跳过因果链创建", cause_id);
            return;
        }

        if let Some(node) = self.graph_db.get_node(effect_id) {
            self.graph_db.add_node(GraphType::Causal, node.clone());
        } else {
            warn!("结果节点 {} 不存在，跳过因果链创建", effect_id);
            return;
        }

        // 创建因果边
        let edge = GraphEdge::new(cause_id, effect_id, EdgeType::Causal, confidence);
        self.graph_db.add_edge(GraphType::Causal, edge);
    }

    /// 添加语义关联
    ///
    /// 在语义图中创建两个节点之间的语义相似边
    pub async fn add_semantic_link(&mut self, node_a_id: &str, node_b_id: &str, similarity: f64) {
        debug!(
            "添加语义关联: {} <-> {} (相似度: {:.2})",
            node_a_id, node_b_id, similarity
        );

        // 确保两个节点都存在于语义图中
        if let Some(node) = self.graph_db.get_node(node_a_id) {
            self.graph_db.add_node(GraphType::Semantic, node.clone());
        }
        if let Some(node) = self.graph_db.get_node(node_b_id) {
            self.graph_db.add_node(GraphType::Semantic, node.clone());
        }

        // 创建双向语义边
        let edge_ab = GraphEdge::new(node_a_id, node_b_id, EdgeType::SemanticSimilarity, similarity);
        let edge_ba = GraphEdge::new(node_b_id, node_a_id, EdgeType::SemanticSimilarity, similarity);

        self.graph_db.add_edge(GraphType::Semantic, edge_ab);
        self.graph_db.add_edge(GraphType::Semantic, edge_ba);
    }

    /// 获取引擎统计信息
    pub fn stats(&self) -> EngineStats {
        let (global_count, graph_stats) = self.graph_db.global_stats();
        EngineStats {
            total_nodes: global_count,
            pending_integration: self.pending_nodes.len(),
            vector_store_size: self.vector_store.len(),
            semantic_graph: graph_stats.get(&GraphType::Semantic).copied().unwrap_or((0, 0)),
            temporal_graph: graph_stats.get(&GraphType::Temporal).copied().unwrap_or((0, 0)),
            causal_graph: graph_stats.get(&GraphType::Causal).copied().unwrap_or((0, 0)),
            entity_graph: graph_stats.get(&GraphType::Entity).copied().unwrap_or((0, 0)),
        }
    }

    /// 获取待整合节点数量
    pub fn pending_count(&self) -> usize {
        self.pending_nodes.len()
    }

    /// 尝试为新节点建立时间边
    /// 查找时间图中最新的节点，建立时间顺序关系
    async fn try_link_temporal(&mut self, new_id: &str) {
        // 获取时间图中的所有节点，按时间排序
        let mut temporal_nodes: Vec<MemoryNode> = self
            .graph_db
            .get_all_nodes(GraphType::Temporal)
            .into_iter()
            .filter(|n| n.id != new_id)
            .cloned()
            .collect();

        temporal_nodes.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

        // 与最近的几个节点建立时间边
        for recent_node in temporal_nodes.iter().take(3) {
            let time_diff = chrono::Utc::now()
                .signed_duration_since(recent_node.timestamp)
                .num_seconds()
                .abs();

            // 只与时间相近的节点建立边（1小时内）
            if time_diff <= 3600 {
                let weight = 1.0 - (time_diff as f64 / 3600.0) * 0.5;

                let edge = GraphEdge::new(
                    recent_node.id.clone(),
                    new_id,
                    EdgeType::TemporalBefore,
                    weight,
                );
                self.graph_db.add_edge(GraphType::Temporal, edge);
            }
        }
    }

    /// 确保实体节点存在
    /// 如果不存在则创建新的实体节点
    async fn ensure_entity_node(&mut self, entity_name: &str) -> String {
        // 在实体图中查找是否已存在该实体
        for node in self.graph_db.get_all_nodes(GraphType::Entity) {
            if node.content == entity_name
                || node
                    .attributes
                    .get("entity_name")
                    .and_then(|v| v.as_str())
                    .map(|s| s == entity_name)
                    .unwrap_or(false)
            {
                return node.id.clone();
            }
        }

        // 创建新的实体节点
        let mut node = MemoryNode::new(entity_name, MemoryNodeType::Observation);
        node.attributes.insert(
            "entity_name".to_string(),
            serde_json::Value::String(entity_name.to_string()),
        );
        node.attributes.insert(
            "is_entity".to_string(),
            serde_json::Value::Bool(true),
        );

        let id = node.id.clone();

        // 添加到实体图
        self.graph_db.add_node(GraphType::Entity, node.clone());

        // 添加到全局索引
        let embedding = self.generate_embedding(entity_name);
        node.embedding = Some(embedding.clone());

        let mut metadata = HashMap::new();
        metadata.insert(
            "content".to_string(),
            serde_json::Value::String(entity_name.to_string()),
        );
        metadata.insert(
            "is_entity".to_string(),
            serde_json::Value::Bool(true),
        );
        self.vector_store.add(id.clone(), embedding, metadata);

        id
    }

    /// 生成文本的伪嵌入向量
    /// 在实际系统中应调用嵌入模型（如 text-embedding-ada-002）
    fn generate_embedding(&self, text: &str) -> Vec<f64> {
        let dimensions = 128;
        let mut embedding = vec![0.0f64; dimensions];

        // 使用字符哈希生成伪向量
        for (i, ch) in text.chars().enumerate() {
            let idx = (ch as usize + i) % dimensions;
            embedding[idx] += (ch as u32 as f64) / 256.0;
        }

        // 归一化
        let norm: f64 = embedding.iter().map(|x| x * x).sum::<f64>().sqrt();
        if norm > 0.0 {
            for v in &mut embedding {
                *v /= norm;
            }
        }

        embedding
    }
}

/// 引擎统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineStats {
    /// 全局节点总数
    pub total_nodes: usize,
    /// 待整合节点数
    pub pending_integration: usize,
    /// 向量存储大小
    pub vector_store_size: usize,
    /// 语义图统计 (节点数, 边数)
    pub semantic_graph: (usize, usize),
    /// 时间图统计 (节点数, 边数)
    pub temporal_graph: (usize, usize),
    /// 因果图统计 (节点数, 边数)
    pub causal_graph: (usize, usize),
    /// 实体图统计 (节点数, 边数)
    pub entity_graph: (usize, usize),
}

impl std::fmt::Display for EngineStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "MAGMA 记忆引擎统计")?;
        writeln!(f, "  全局节点: {}", self.total_nodes)?;
        writeln!(f, "  待整合: {}", self.pending_integration)?;
        writeln!(f, "  向量存储: {}", self.vector_store_size)?;
        writeln!(
            f,
            "  语义图: {} 节点, {} 边",
            self.semantic_graph.0, self.semantic_graph.1
        )?;
        writeln!(
            f,
            "  时间图: {} 节点, {} 边",
            self.temporal_graph.0, self.temporal_graph.1
        )?;
        writeln!(
            f,
            "  因果图: {} 节点, {} 边",
            self.causal_graph.0, self.causal_graph.1
        )?;
        writeln!(
            f,
            "  实体图: {} 节点, {} 边",
            self.entity_graph.0, self.entity_graph.1
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_engine_creation() {
        let config = MemoryConfig::default();
        let engine = MagmaMemoryEngine::new(&config).unwrap();
        let stats = engine.stats();
        assert_eq!(stats.total_nodes, 0);
        assert_eq!(stats.pending_integration, 0);
    }

    #[tokio::test]
    async fn test_fast_ingest() {
        let config = MemoryConfig::default();
        let mut engine = MagmaMemoryEngine::new(&config).unwrap();

        let id = engine
            .ingest_fast("今天天气很好", MemoryNodeType::Observation)
            .await
            .unwrap();

        assert!(!id.is_empty());
        let stats = engine.stats();
        assert_eq!(stats.pending_integration, 1);
    }

    #[tokio::test]
    async fn test_retrieve() {
        let config = MemoryConfig::default();
        let mut engine = MagmaMemoryEngine::new(&config).unwrap();

        engine
            .ingest_fast("Rust 是一种系统编程语言", MemoryNodeType::Observation)
            .await
            .unwrap();

        let result = engine.retrieve("Rust", None).await.unwrap();
        // 由于使用伪嵌入，结果可能为空
        // 这里主要测试不会 panic
    }

    #[tokio::test]
    async fn test_entity_relation() {
        let config = MemoryConfig::default();
        let mut engine = MagmaMemoryEngine::new(&config).unwrap();

        engine
            .add_entity_relation("北京", "中国", "位于")
            .await;

        let stats = engine.stats();
        assert!(stats.entity_graph.0 >= 2);
        assert!(stats.entity_graph.1 >= 1);
    }

    #[tokio::test]
    async fn test_causal_link() {
        let config = MemoryConfig::default();
        let mut engine = MagmaMemoryEngine::new(&config).unwrap();

        let cause_id = engine
            .ingest_fast("下雨了", MemoryNodeType::Observation)
            .await
            .unwrap();

        let effect_id = engine
            .ingest_fast("地面湿了", MemoryNodeType::StateChange)
            .await
            .unwrap();

        engine
            .add_causal_link(&cause_id, &effect_id, 0.9)
            .await;

        let stats = engine.stats();
        assert!(stats.causal_graph.1 >= 1);
    }

    #[tokio::test]
    async fn test_slow_integration() {
        let config = MemoryConfig::default();
        let mut engine = MagmaMemoryEngine::new(&config).unwrap();

        engine
            .ingest_fast("测试记忆1", MemoryNodeType::Observation)
            .await
            .unwrap();
        engine
            .ingest_fast("测试记忆2", MemoryNodeType::Action)
            .await
            .unwrap();

        let report = engine.integrate_slow().await.unwrap();
        assert_eq!(engine.pending_count(), 0);
    }
}
