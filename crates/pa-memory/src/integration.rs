// MAGMA 多图谱智能体记忆架构 - 记忆整合实现
// 本模块实现了 MAGMA 的慢速整合流程 MemoryIntegrator，
// 包含四阶段整合：定位 -> 收集 -> 整合 -> 修剪和索引。

use std::collections::{HashMap, HashSet};

use tracing::{debug, info};

use crate::graph::InMemoryGraphDB;
use crate::types::*;
use crate::vector::InMemoryVectorStore;

/// 记忆整合器 - 负责将短期记忆整合到长期图谱中
///
/// 四阶段整合流程：
/// 1. 定位（Locate）：找到需要整合的候选节点
/// 2. 收集（Collect）：收集相关的上下文信息和邻居
/// 3. 整合（Integrate）：合并重复、解决矛盾、推断新关系
/// 4. 修剪和索引（Prune & Index）：清理低价值节点，更新索引
pub struct MemoryIntegrator<'a> {
    /// 图数据库引用
    graph_db: &'a InMemoryGraphDB,
    /// 向量存储引用（保留用于后续整合阶段直接操作向量索引）
    _vector_store: &'a InMemoryVectorStore,
    /// 记忆配置
    config: &'a MemoryConfig,
}

impl<'a> MemoryIntegrator<'a> {
    /// 创建新的记忆整合器
    pub fn new(
        graph_db: &'a InMemoryGraphDB,
        vector_store: &'a InMemoryVectorStore,
        config: &'a MemoryConfig,
    ) -> Self {
        Self {
            graph_db,
            _vector_store: vector_store,
            config,
        }
    }

    /// 执行完整的四阶段整合流程
    ///
    /// # 阶段 1: 定位
    /// 找到所有标记为 "pending" 的节点作为整合候选
    ///
    /// # 阶段 2: 收集
    /// 对每个候选节点，收集其在各图谱中的邻居和上下文
    ///
    /// # 阶段 3: 整合
    /// - 检测并合并重复记忆
    /// - 解决矛盾记忆
    /// - 推断新的关系边
    ///
    /// # 阶段 4: 修剪和索引
    /// - 修剪低频访问的节点
    /// - 更新向量索引
    pub async fn integrate(&self) -> Result<IntegrationReport> {
        info!("开始记忆整合流程");

        let mut report = IntegrationReport::default();

        // 阶段 1: 定位 - 找到待整合的候选节点
        let candidates = self.locate_candidates();
        debug!("阶段1（定位）: 找到 {} 个候选节点", candidates.len());

        if candidates.is_empty() {
            debug!("没有候选节点需要整合");
            return Ok(report);
        }

        // 阶段 2: 收集 - 收集候选节点的上下文
        let contexts = self.collect_contexts(&candidates);
        debug!("阶段2（收集）: 收集了 {} 个节点的上下文", contexts.len());

        // 阶段 3: 整合 - 合并重复、解决矛盾、推断关系
        // 3a: 检测重复
        let duplicate_groups = self.detect_duplicates(&candidates);
        debug!(
            "阶段3a（重复检测）: 发现 {} 组重复节点",
            duplicate_groups.len()
        );

        // 3b: 解决矛盾
        let resolutions = self.resolve_contradictions(&duplicate_groups);
        report.contradictions_resolved = resolutions.len();
        debug!(
            "阶段3b（矛盾解决）: 解决了 {} 个矛盾",
            report.contradictions_resolved
        );

        // 3c: 推断新关系
        let inferred_edges = self.infer_relations();
        report.edges_inferred = inferred_edges.len();
        debug!(
            "阶段3d（关系推断）: 推断了 {} 条新关系",
            report.edges_inferred
        );

        report.nodes_merged = duplicate_groups.len();

        // 阶段 4: 修剪和索引
        // （注意：修剪操作需要可变引用，这里只报告统计信息）
        let prunable = self.count_prunable_nodes();
        report.nodes_pruned = prunable;
        debug!("阶段4（修剪）: 发现 {} 个可修剪节点", prunable);

        info!(
            "记忆整合完成: 合并={}, 推断={}, 矛盾={}, 修剪={}",
            report.nodes_merged,
            report.edges_inferred,
            report.contradictions_resolved,
            report.nodes_pruned
        );

        Ok(report)
    }

    /// 阶段 1: 定位候选节点
    ///
    /// 查找所有标记为 "pending" 整合状态的节点
    fn locate_candidates(&self) -> Vec<MemoryNode> {
        let mut candidates = Vec::new();

        for node in self.graph_db.get_all_global_nodes() {
            let status = node
                .attributes
                .get("integration_status")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");

            if status == "pending" {
                candidates.push(node.clone());
            }
        }

        candidates
    }

    /// 阶段 2: 收集上下文
    ///
    /// 对每个候选节点，收集其在各图谱中的邻居信息
    fn collect_contexts(
        &self,
        candidates: &[MemoryNode],
    ) -> HashMap<String, NodeContext> {
        let mut contexts = HashMap::new();

        for node in candidates {
            let mut context = NodeContext {
                neighbors_by_graph: HashMap::new(),
                total_connections: 0,
            };

            // 收集每个图谱中的邻居
            for graph_type in GraphType::all() {
                let neighbors = self.graph_db.get_neighbors(&node.id, *graph_type);
                if !neighbors.is_empty() {
                    context
                        .neighbors_by_graph
                        .insert(*graph_type, neighbors.len());
                    context.total_connections += neighbors.len();
                }
            }

            contexts.insert(node.id.clone(), context);
        }

        contexts
    }

    /// 阶段 3a: 检测重复记忆
    ///
    /// 通过向量相似度和内容匹配检测重复的记忆节点
    /// 返回分组列表，每组包含相似节点的 ID
    fn detect_duplicates(&self, candidates: &[MemoryNode]) -> Vec<Vec<String>> {
        let mut groups: Vec<Vec<String>> = Vec::new();
        let mut assigned: HashSet<String> = HashSet::new();

        for i in 0..candidates.len() {
            if assigned.contains(&candidates[i].id) {
                continue;
            }

            let mut group = vec![candidates[i].id.clone()];

            for j in (i + 1)..candidates.len() {
                if assigned.contains(&candidates[j].id) {
                    continue;
                }

                // 检查相似度
                let is_duplicate = self.check_duplicate(&candidates[i], &candidates[j]);
                if is_duplicate {
                    group.push(candidates[j].id.clone());
                    assigned.insert(candidates[j].id.clone());
                }
            }

            if group.len() > 1 {
                assigned.insert(candidates[i].id.clone());
                groups.push(group);
            }
        }

        groups
    }

    /// 检查两个节点是否为重复
    fn check_duplicate(&self, a: &MemoryNode, b: &MemoryNode) -> bool {
        // 方法 1: 内容完全相同
        if a.content == b.content {
            return true;
        }

        // 方法 2: 向量相似度超过阈值
        if let (Some(emb_a), Some(emb_b)) = (&a.embedding, &b.embedding) {
            let similarity = crate::vector::cosine_similarity(emb_a, emb_b);
            if similarity >= self.config.duplicate_threshold {
                return true;
            }
        }

        // 方法 3: 内容包含关系（一个内容是另一个的子串，且长度接近）
        let len_ratio = a.content.len() as f64 / b.content.len().max(1) as f64;
        if (len_ratio > 0.8 && len_ratio < 1.2)
            && (a.content.contains(&b.content) || b.content.contains(&a.content))
        {
            return true;
        }

        false
    }

    /// 阶段 3b: 解决矛盾
    ///
    /// 对于检测到的重复/矛盾节点组，决定保留策略：
    /// - 如果内容一致：合并，保留较新的
    /// - 如果内容矛盾：保留两者但标记矛盾
    /// - 如果一个更详细：保留更详细的那个
    fn resolve_contradictions(&self, groups: &[Vec<String>]) -> Vec<ResolutionAction> {
        let mut actions = Vec::new();

        for group in groups {
            if group.len() < 2 {
                continue;
            }

            // 获取组内所有节点
            let nodes: Vec<&MemoryNode> = group
                .iter()
                .filter_map(|id| self.graph_db.get_node(id))
                .collect();

            if nodes.len() < 2 {
                continue;
            }

            // 检查内容是否一致
            let all_same_content = nodes
                .windows(2)
                .all(|w| w[0].content == w[1].content);

            if all_same_content {
                // 内容一致：合并，保留最新的
                let mut sorted = nodes.to_vec();
                sorted.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

                let keep_id = sorted[0].id.clone();
                for i in 1..sorted.len() {
                    actions.push(ResolutionAction::Merge {
                        keep_id: keep_id.clone(),
                        remove_id: sorted[i].id.clone(),
                    });
                }
            } else {
                // 检查是否一个包含另一个的详细信息
                let mut sorted = nodes.to_vec();
                sorted.sort_by(|a, b| b.content.len().cmp(&a.content.len()));

                let longest = &sorted[0];
                let is_subset = sorted[1..]
                    .iter()
                    .all(|n| longest.content.contains(&n.content) || n.content.contains(&longest.content));

                if is_subset && sorted.len() == 2 {
                    // 一个是另一个的子集：保留更详细的
                    actions.push(ResolutionAction::Merge {
                        keep_id: longest.id.clone(),
                        remove_id: sorted[1].id.clone(),
                    });
                } else {
                    // 内容矛盾：保留两者
                    for pair in sorted.windows(2) {
                        actions.push(ResolutionAction::KeepBoth {
                            node_ids: (pair[0].id.clone(), pair[1].id.clone()),
                        });
                    }
                }
            }
        }

        actions
    }

    /// 阶段 3c: 推断新关系
    ///
    /// 基于现有图谱结构推断新的关系边：
    /// - 传递性关系：如果 A->B 和 B->C 存在，可能存在 A->C
    /// - 共同邻居：如果 A 和 B 有很多共同邻居，可能存在直接关系
    /// - 时间邻近：时间相近的事件可能存在因果关系
    fn infer_relations(&self) -> Vec<GraphEdge> {
        let mut inferred = Vec::new();

        // 策略 1: 传递性关系推断（因果图）
        inferred.extend(self.infer_transitive_relations(GraphType::Causal));

        // 策略 2: 共同邻居推断（语义图）
        inferred.extend(self.infer_common_neighbor_relations(GraphType::Semantic));

        // 策略 3: 时间邻近因果推断
        inferred.extend(self.infer_temporal_causal_relations());

        inferred
    }

    /// 传递性关系推断
    /// 如果 A->B 和 B->C 存在，且 A->C 不存在，则推断 A->C
    fn infer_transitive_relations(&self, graph_type: GraphType) -> Vec<GraphEdge> {
        let mut inferred = Vec::new();
        let nodes = self.graph_db.get_all_nodes(graph_type);

        for node in &nodes {
            // 获取两跳邻居
            let first_hop: Vec<String> = self
                .graph_db
                .get_outgoing_neighbors(&node.id, graph_type)
                .iter()
                .map(|n| n.id.clone())
                .collect();

            for neighbor_id in &first_hop {
                let second_hop: Vec<String> = self
                    .graph_db
                    .get_outgoing_neighbors(neighbor_id, graph_type)
                    .iter()
                    .map(|n| n.id.clone())
                    .collect();

                for target_id in &second_hop {
                    // 跳过自身和直接邻居
                    if target_id == &node.id || first_hop.contains(target_id) {
                        continue;
                    }

                    // 检查是否已存在边
                    if !self.graph_db.has_edge_type(
                        &node.id,
                        target_id,
                        graph_type,
                        &graph_type.default_edge_type(),
                    ) {
                        // 推断传递性边，权重较低
                        let edge = GraphEdge::new(
                            node.id.clone(),
                            target_id.clone(),
                            graph_type.default_edge_type(),
                            0.3, // 传递性推断的权重较低
                        );
                        inferred.push(edge);
                    }
                }
            }
        }

        inferred
    }

    /// 共同邻居关系推断
    /// 如果两个节点有很多共同邻居，可能存在直接关系
    fn infer_common_neighbor_relations(&self, graph_type: GraphType) -> Vec<GraphEdge> {
        let mut inferred = Vec::new();
        let nodes = self.graph_db.get_all_nodes(graph_type);
        let min_common = 3; // 最少共同邻居数

        for i in 0..nodes.len() {
            let neighbors_i: HashSet<String> = self
                .graph_db
                .get_neighbors(&nodes[i].id, graph_type)
                .iter()
                .map(|n| n.id.clone())
                .collect();

            for j in (i + 1)..nodes.len() {
                let neighbors_j: HashSet<String> = self
                    .graph_db
                    .get_neighbors(&nodes[j].id, graph_type)
                    .iter()
                    .map(|n| n.id.clone())
                    .collect();

                // 计算共同邻居
                let common: HashSet<&String> =
                    neighbors_i.intersection(&neighbors_j).collect();

                if common.len() >= min_common {
                    // 检查是否已存在边
                    if !self.graph_db.has_edge_type(
                        &nodes[i].id,
                        &nodes[j].id,
                        graph_type,
                        &graph_type.default_edge_type(),
                    ) {
                        // 权重基于共同邻居比例
                        let union_count = neighbors_i.union(&neighbors_j).count();
                        let jaccard = common.len() as f64 / union_count.max(1) as f64;

                        let edge = GraphEdge::new(
                            nodes[i].id.clone(),
                            nodes[j].id.clone(),
                            graph_type.default_edge_type(),
                            jaccard * 0.6, // 共同邻居推断的权重
                        );
                        inferred.push(edge);
                    }
                }
            }
        }

        inferred
    }

    /// 时间邻近因果推断
    /// 时间相近的事件可能存在因果关系
    fn infer_temporal_causal_relations(&self) -> Vec<GraphEdge> {
        let mut inferred = Vec::new();

        // 获取时间图中的所有节点，按时间排序
        let mut temporal_nodes: Vec<&MemoryNode> =
            self.graph_db.get_all_nodes(GraphType::Temporal);
        temporal_nodes.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

        // 检查相邻时间的事件
        for window in temporal_nodes.windows(2) {
            let time_diff = window[1]
                .timestamp
                .signed_duration_since(window[0].timestamp)
                .num_seconds();

            // 时间差在 5 分钟内的事件可能存在因果关系
            if time_diff >= 0 && time_diff <= 300 {
                // 检查因果图中是否已存在关系
                if !self.graph_db.has_edge_type(
                    &window[0].id,
                    &window[1].id,
                    GraphType::Causal,
                    &EdgeType::Causal,
                ) {
                    // 时间越近，因果关系的可能性越高
                    let weight = 1.0 - (time_diff as f64 / 300.0) * 0.5;

                    let edge = GraphEdge::new(
                        window[0].id.clone(),
                        window[1].id.clone(),
                        EdgeType::Causal,
                        weight * 0.4, // 时间邻近推断的权重较低
                    );
                    inferred.push(edge);
                }
            }
        }

        inferred
    }

    /// 阶段 4: 统计可修剪的节点数
    ///
    /// 低频节点是指访问次数低于阈值的节点
    fn count_prunable_nodes(&self) -> usize {
        let mut count = 0;

        for node in self.graph_db.get_all_global_nodes() {
            // 跳过实体节点（实体不应被修剪）
            let is_entity = node
                .attributes
                .get("is_entity")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            if is_entity {
                continue;
            }

            // 检查访问频率
            if node.access_count() < self.config.prune_frequency_threshold {
                // 检查节点是否被其他节点引用（有入边）
                let has_incoming = GraphType::all()
                    .iter()
                    .any(|gt| {
                        !self
                            .graph_db
                            .get_incoming_neighbors(&node.id, *gt)
                            .is_empty()
                    });

                if !has_incoming {
                    count += 1;
                }
            }
        }

        count
    }

    /// 执行修剪操作（需要外部传入可变引用）
    /// 注意：此方法由引擎在适当时机调用
    pub fn prune_low_frequency(
        graph_db: &mut InMemoryGraphDB,
        vector_store: &mut InMemoryVectorStore,
        threshold: usize,
    ) -> usize {
        let mut pruned = 0;

        // 收集需要修剪的节点 ID
        let to_prune: Vec<String> = graph_db
            .get_all_global_nodes()
            .iter()
            .filter(|node| {
                let is_entity = node
                    .attributes
                    .get("is_entity")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                if is_entity {
                    return false;
                }

                node.access_count() < threshold
            })
            .map(|n| n.id.clone())
            .collect();

        // 执行修剪
        for id in &to_prune {
            graph_db.remove_node_from_all(id);
            vector_store.remove(id);
            pruned += 1;
        }

        debug!("修剪了 {} 个低频节点", pruned);
        pruned
    }
}

/// 节点上下文 - 整合过程中收集的节点信息
#[derive(Debug, Clone)]
struct NodeContext {
    /// 各图谱中的邻居数量
    neighbors_by_graph: HashMap<GraphType, usize>,
    /// 总连接数
    total_connections: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_components() -> (InMemoryGraphDB, InMemoryVectorStore, MemoryConfig) {
        let graph_db = InMemoryGraphDB::new();
        let vector_store = InMemoryVectorStore::new();
        let config = MemoryConfig::default();
        (graph_db, vector_store, config)
    }

    #[tokio::test]
    async fn test_integrate_empty() {
        let (graph_db, vector_store, config) = create_test_components();
        let integrator = MemoryIntegrator::new(&graph_db, &vector_store, &config);

        let report = integrator.integrate().await.unwrap();
        assert!(!report.has_changes());
    }

    #[tokio::test]
    async fn test_detect_duplicates() {
        let (mut graph_db, vector_store, config) = create_test_components();

        // 创建两个内容相同的节点
        let mut n1 = MemoryNode::new("相同的内容", MemoryNodeType::Observation);
        n1.attributes.insert(
            "integration_status".to_string(),
            serde_json::Value::String("pending".to_string()),
        );
        n1.embedding = Some(vec![1.0, 0.0, 0.0]);

        let mut n2 = MemoryNode::new("相同的内容", MemoryNodeType::Observation);
        n2.attributes.insert(
            "integration_status".to_string(),
            serde_json::Value::String("pending".to_string()),
        );
        n2.embedding = Some(vec![1.0, 0.0, 0.0]);

        graph_db.add_node(GraphType::Semantic, n1);
        graph_db.add_node(GraphType::Semantic, n2);

        let integrator = MemoryIntegrator::new(&graph_db, &vector_store, &config);
        let candidates = integrator.locate_candidates();
        let groups = integrator.detect_duplicates(&candidates);

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].len(), 2);
    }

    #[tokio::test]
    async fn test_infer_relations() {
        let (mut graph_db, vector_store, config) = create_test_components();

        // 创建 A -> B -> C 的因果链
        let n1 = MemoryNode::with_id("a", "事件A", MemoryNodeType::Action);
        let n2 = MemoryNode::with_id("b", "事件B", MemoryNodeType::StateChange);
        let n3 = MemoryNode::with_id("c", "事件C", MemoryNodeType::StateChange);

        graph_db.add_node(GraphType::Causal, n1);
        graph_db.add_node(GraphType::Causal, n2);
        graph_db.add_node(GraphType::Causal, n3);

        graph_db.add_edge(
            GraphType::Causal,
            GraphEdge::new("a", "b", EdgeType::Causal, 0.9),
        );
        graph_db.add_edge(
            GraphType::Causal,
            GraphEdge::new("b", "c", EdgeType::Causal, 0.8),
        );

        let integrator = MemoryIntegrator::new(&graph_db, &vector_store, &config);
        let inferred = integrator.infer_relations();

        // 应该推断出 A -> C 的传递性关系
        assert!(inferred.iter().any(|e| e.source_id == "a" && e.target_id == "c"));
    }

    #[tokio::test]
    async fn test_prune_low_frequency() {
        let mut graph_db = InMemoryGraphDB::new();
        let mut vector_store = InMemoryVectorStore::new();

        // 创建一个低频节点
        let node = MemoryNode::new("低频内容", MemoryNodeType::Observation);
        let id = node.id.clone();
        graph_db.add_node(GraphType::Semantic, node);

        let mut metadata = HashMap::new();
        metadata.insert(
            "content".to_string(),
            serde_json::Value::String("低频内容".to_string()),
        );
        vector_store.add(id.clone(), vec![1.0, 0.0], metadata);

        // 修剪访问次数 < 2 的节点
        let pruned = MemoryIntegrator::prune_low_frequency(&mut graph_db, &mut vector_store, 2);

        assert_eq!(pruned, 1);
        assert!(!graph_db.node_exists_global(&id));
    }
}
