// MAGMA 多图谱智能体记忆架构 - 图数据库实现
// 本模块实现了内存图数据库 InMemoryGraphDB，支持四个正交图谱的独立管理和跨图谱遍历。

use std::collections::{HashMap, HashSet, VecDeque};

use tracing::{debug, info};

use crate::types::*;

/// 单个图谱的邻接表结构
/// 使用双向邻接表存储，支持高效的正向和反向遍历
#[derive(Debug, Clone)]
struct GraphLayer {
    /// 节点 ID 到节点数据的映射
    nodes: HashMap<String, MemoryNode>,
    /// 正向邻接表：节点 ID -> 出边列表
    outgoing: HashMap<String, Vec<GraphEdge>>,
    /// 反向邻接表：节点 ID -> 入边列表
    incoming: HashMap<String, Vec<GraphEdge>>,
}

impl GraphLayer {
    /// 创建空的图谱层
    fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            outgoing: HashMap::new(),
            incoming: HashMap::new(),
        }
    }

    /// 添加节点到图谱层
    fn add_node(&mut self, node: MemoryNode) {
        let id = node.id.clone();
        // 如果节点已存在，更新内容
        if self.nodes.contains_key(&id) {
            debug!("更新图谱层中已存在的节点: {}", id);
        }
        self.nodes.insert(id.clone(), node);
        // 确保邻接表中有该节点的条目
        self.outgoing.entry(id.clone()).or_default();
        self.incoming.entry(id).or_default();
    }

    /// 添加边到图谱层
    fn add_edge(&mut self, edge: GraphEdge) {
        let source = edge.source_id.clone();
        let target = edge.target_id.clone();

        // 确保两个端点节点都存在于邻接表中
        self.outgoing.entry(source.clone()).or_default();
        self.outgoing.entry(target.clone()).or_default();
        self.incoming.entry(source).or_default();
        self.incoming.entry(target.clone()).or_default();

        // 添加正向边
        self.outgoing
            .get_mut(&edge.source_id)
            .unwrap()
            .push(edge.clone());

        // 添加反向边
        self.incoming
            .get_mut(&edge.target_id)
            .unwrap()
            .push(edge);
    }

    /// 获取节点
    fn get_node(&self, id: &str) -> Option<&MemoryNode> {
        self.nodes.get(id)
    }

    /// 获取节点的所有出边邻居
    fn get_outgoing_neighbors(&self, id: &str) -> Vec<&MemoryNode> {
        self.outgoing
            .get(id)
            .map(|edges| {
                edges
                    .iter()
                    .filter_map(|e| self.nodes.get(&e.target_id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// 获取节点的所有入边邻居
    fn get_incoming_neighbors(&self, id: &str) -> Vec<&MemoryNode> {
        self.incoming
            .get(id)
            .map(|edges| {
                edges
                    .iter()
                    .filter_map(|e| self.nodes.get(&e.source_id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// 获取节点的所有邻居（双向）
    fn get_all_neighbors(&self, id: &str) -> Vec<&MemoryNode> {
        let mut neighbors = HashSet::new();
        for edge in self.outgoing.get(id).unwrap_or(&Vec::new()) {
            if let Some(node) = self.nodes.get(&edge.target_id) {
                neighbors.insert(node.id.clone());
            }
        }
        for edge in self.incoming.get(id).unwrap_or(&Vec::new()) {
            if let Some(node) = self.nodes.get(&edge.source_id) {
                neighbors.insert(node.id.clone());
            }
        }
        neighbors
            .into_iter()
            .filter_map(|nid| self.nodes.get(&nid))
            .collect()
    }

    /// 获取从指定节点出发的所有出边
    fn get_outgoing_edges(&self, id: &str) -> &[GraphEdge] {
        self.outgoing.get(id).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// 删除节点及其所有关联的边
    fn remove_node(&mut self, id: &str) -> Option<MemoryNode> {
        // 收集所有需要删除的边
        let outgoing_edges: Vec<GraphEdge> = self
            .outgoing
            .get(id)
            .cloned()
            .unwrap_or_default();

        let incoming_edges: Vec<GraphEdge> = self
            .incoming
            .get(id)
            .cloned()
            .unwrap_or_default();

        // 从其他节点的邻接表中移除相关边
        for edge in &outgoing_edges {
            if let Some(incoming_list) = self.incoming.get_mut(&edge.target_id) {
                incoming_list.retain(|e| e.source_id != id);
            }
        }

        for edge in &incoming_edges {
            if let Some(outgoing_list) = self.outgoing.get_mut(&edge.source_id) {
                outgoing_list.retain(|e| e.target_id != id);
            }
        }

        // 移除节点自身的邻接表条目
        self.outgoing.remove(id);
        self.incoming.remove(id);

        // 移除节点
        self.nodes.remove(id)
    }

    /// 获取图谱中的所有节点 ID
    fn node_ids(&self) -> Vec<String> {
        self.nodes.keys().cloned().collect()
    }

    /// 获取图谱中的所有边
    fn all_edges(&self) -> Vec<&GraphEdge> {
        self.outgoing.values().flat_map(|v| v.iter()).collect()
    }

    /// 获取节点数量
    fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// 获取边数量
    fn edge_count(&self) -> usize {
        self.outgoing.values().map(|v| v.len()).sum()
    }
}

/// 内存图数据库 - MAGMA 的核心存储组件
/// 管理四个正交图谱（语义、时间、因果、实体），支持独立和跨图谱操作
#[derive(Debug, Clone)]
pub struct InMemoryGraphDB {
    /// 四个正交图谱层
    layers: HashMap<GraphType, GraphLayer>,
    /// 全局节点索引（跨图谱共享的节点存储）
    /// 注意：同一个节点可以存在于多个图谱中
    global_nodes: HashMap<String, MemoryNode>,
}

impl InMemoryGraphDB {
    /// 创建新的内存图数据库，初始化四个正交图谱
    pub fn new() -> Self {
        let mut layers = HashMap::new();
        for graph_type in GraphType::all() {
            layers.insert(*graph_type, GraphLayer::new());
        }

        info!("初始化 MAGMA 内存图数据库，包含四个正交图谱");

        Self {
            layers,
            global_nodes: HashMap::new(),
        }
    }

    /// 向指定图谱添加节点
    /// 节点同时注册到全局索引中
    pub fn add_node(&mut self, graph_type: GraphType, node: MemoryNode) {
        let id = node.id.clone();
        debug!("向 {} 添加节点: {}", graph_type, id);

        // 注册到全局索引
        if let Some(existing) = self.global_nodes.get_mut(&id) {
            // 合并属性
            for (k, v) in &node.attributes {
                existing.attributes.insert(k.clone(), v.clone());
            }
            // 更新嵌入向量（如果新节点有而旧节点没有）
            if existing.embedding.is_none() && node.embedding.is_some() {
                existing.embedding = node.embedding.clone();
            }
        } else {
            self.global_nodes.insert(id.clone(), node.clone());
        }

        // 添加到指定图谱层
        if let Some(layer) = self.layers.get_mut(&graph_type) {
            layer.add_node(node);
        }
    }

    /// 向所有图谱添加节点
    pub fn add_node_to_all(&mut self, node: MemoryNode) {
        for graph_type in GraphType::all() {
            self.add_node(*graph_type, node.clone());
        }
    }

    /// 向指定图谱添加边
    pub fn add_edge(&mut self, graph_type: GraphType, edge: GraphEdge) {
        debug!(
            "向 {} 添加边: {} -> {} ({})",
            graph_type, edge.source_id, edge.target_id, edge.edge_type
        );

        if let Some(layer) = self.layers.get_mut(&graph_type) {
            layer.add_edge(edge);
        }
    }

    /// 从全局索引获取节点
    pub fn get_node(&self, id: &str) -> Option<&MemoryNode> {
        self.global_nodes.get(id)
    }

    /// 从全局索引获取可变节点引用
    pub fn get_node_mut(&mut self, id: &str) -> Option<&mut MemoryNode> {
        self.global_nodes.get_mut(id)
    }

    /// 获取指定图谱中节点的邻居
    pub fn get_neighbors(&self, id: &str, graph_type: GraphType) -> Vec<&MemoryNode> {
        if let Some(layer) = self.layers.get(&graph_type) {
            layer.get_all_neighbors(id)
        } else {
            Vec::new()
        }
    }

    /// 获取指定图谱中节点的出边邻居
    pub fn get_outgoing_neighbors(&self, id: &str, graph_type: GraphType) -> Vec<&MemoryNode> {
        if let Some(layer) = self.layers.get(&graph_type) {
            layer.get_outgoing_neighbors(id)
        } else {
            Vec::new()
        }
    }

    /// 获取指定图谱中节点的入边邻居
    pub fn get_incoming_neighbors(&self, id: &str, graph_type: GraphType) -> Vec<&MemoryNode> {
        if let Some(layer) = self.layers.get(&graph_type) {
            layer.get_incoming_neighbors(id)
        } else {
            Vec::new()
        }
    }

    /// 在指定图谱中执行 BFS 遍历
    /// 返回从起始节点出发的所有可达路径
    pub fn traverse(
        &self,
        start_id: &str,
        graph_type: GraphType,
        max_hops: usize,
    ) -> Vec<TraversalPath> {
        let mut paths = Vec::new();

        if let Some(layer) = self.layers.get(&graph_type) {
            // 检查起始节点是否存在
            if !layer.nodes.contains_key(start_id) {
                debug!(
                    "遍历起始节点 {} 在 {} 中不存在",
                    start_id, graph_type
                );
                return paths;
            }

            // BFS 遍历
            // 队列元素: (当前节点ID, 当前路径, 路径评分)
            let mut queue: VecDeque<(String, TraversalPath, f64)> = VecDeque::new();
            let mut initial_path = TraversalPath::new();
            initial_path.nodes.push(start_id.to_string());
            queue.push_back((start_id.to_string(), initial_path, 1.0));

            while let Some((current_id, mut current_path, path_score)) = queue.pop_front() {
                let current_hop = current_path.hop_count();

                // 如果已达到最大跳数，不再扩展
                if current_hop >= max_hops {
                    if current_hop > 0 {
                        paths.push(current_path);
                    }
                    continue;
                }

                // 获取出边
                let edges = layer.get_outgoing_edges(&current_id);
                let mut has_children = false;

                for edge in edges {
                    // 避免环路
                    if current_path.nodes.contains(&edge.target_id) {
                        continue;
                    }

                    has_children = true;
                    let mut new_path = current_path.clone();
                    new_path.push(edge.target_id.clone(), edge.edge_type.clone());
                    new_path.score = path_score * edge.weight;

                    queue.push_back((
                        edge.target_id.clone(),
                        new_path,
                        path_score * edge.weight,
                    ));
                }

                // 如果没有子节点且路径长度 > 0，添加为有效路径
                if !has_children && current_hop > 0 {
                    paths.push(current_path);
                }
            }
        }

        // 按评分降序排序
        paths.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        debug!(
            "在 {} 中从 {} 遍历 {} 跳，发现 {} 条路径",
            graph_type,
            start_id,
            max_hops,
            paths.len()
        );

        paths
    }

    /// 在指定图谱中执行带边类型过滤的 BFS 遍历
    pub fn traverse_filtered(
        &self,
        start_id: &str,
        graph_type: GraphType,
        max_hops: usize,
        edge_filter: &[EdgeType],
    ) -> Vec<TraversalPath> {
        let mut paths = Vec::new();

        if let Some(layer) = self.layers.get(&graph_type) {
            if !layer.nodes.contains_key(start_id) {
                return paths;
            }

            let mut queue: VecDeque<(String, TraversalPath, f64)> = VecDeque::new();
            let mut initial_path = TraversalPath::new();
            initial_path.nodes.push(start_id.to_string());
            queue.push_back((start_id.to_string(), initial_path, 1.0));

            while let Some((current_id, mut current_path, path_score)) = queue.pop_front() {
                let current_hop = current_path.hop_count();

                if current_hop >= max_hops {
                    if current_hop > 0 {
                        paths.push(current_path);
                    }
                    continue;
                }

                let edges = layer.get_outgoing_edges(&current_id);
                let mut has_children = false;

                for edge in edges {
                    // 边类型过滤
                    if !edge_filter.is_empty() && !edge_filter.contains(&edge.edge_type) {
                        continue;
                    }

                    if current_path.nodes.contains(&edge.target_id) {
                        continue;
                    }

                    has_children = true;
                    let mut new_path = current_path.clone();
                    new_path.push(edge.target_id.clone(), edge.edge_type.clone());
                    new_path.score = path_score * edge.weight;

                    queue.push_back((
                        edge.target_id.clone(),
                        new_path,
                        path_score * edge.weight,
                    ));
                }

                if !has_children && current_hop > 0 {
                    paths.push(current_path);
                }
            }
        }

        paths.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        paths
    }

    /// 多图谱遍历 - 根据查询意图在多个图谱中执行遍历
    /// 按意图优先级依次查询，合并结果
    pub fn multi_graph_traverse(
        &self,
        start_id: &str,
        intent: QueryIntent,
        max_hops: usize,
    ) -> Vec<TraversalPath> {
        let priority_graphs = intent.priority_graphs();
        let mut all_paths = Vec::new();

        for graph_type in &priority_graphs {
            let graph_max_hops = max_hops.min(intent.max_hops());
            let mut paths = self.traverse(start_id, *graph_type, graph_max_hops);

            // 为来自优先级更高的图谱的路径增加评分
            let priority_boost = 1.0 + (priority_graphs.len() - all_paths.len()) as f64 * 0.1;
            for path in &mut paths {
                path.score *= priority_boost;
            }

            all_paths.extend(paths);
        }

        // 去重（基于路径末尾节点）并按评分排序
        all_paths.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        all_paths.dedup_by(|a, b| {
            if let (Some(a_last), Some(b_last)) = (a.nodes.last(), b.nodes.last()) {
                a_last == b_last
            } else {
                false
            }
        });

        debug!(
            "多图谱遍历从 {} 出发（意图: {}），发现 {} 条路径",
            start_id,
            intent,
            all_paths.len()
        );

        all_paths
    }

    /// 提取子图 - 给定一组节点 ID，返回包含这些节点及其之间所有边的子图
    pub fn find_subgraph(
        &self,
        node_ids: &[String],
    ) -> HashMap<String, (MemoryNode, Vec<GraphEdge>)> {
        let mut subgraph = HashMap::new();
        let node_set: HashSet<&String> = node_ids.iter().collect();

        for graph_type in GraphType::all() {
            if let Some(layer) = self.layers.get(graph_type) {
                for edge in layer.all_edges() {
                    let source_in = node_set.contains(&edge.source_id);
                    let target_in = node_set.contains(&edge.target_id);

                    if source_in || target_in {
                        // 确保两个端点都在子图中
                        if source_in {
                            let entry = subgraph
                                .entry(edge.source_id.clone())
                                .or_insert_with(|| {
                                    let node = self
                                        .global_nodes
                                        .get(&edge.source_id)
                                        .cloned()
                                        .unwrap_or_else(|| {
                                            MemoryNode::with_id(
                                                &edge.source_id,
                                                "未知节点",
                                                MemoryNodeType::Inferred,
                                            )
                                        });
                                    (node, Vec::new())
                                });
                            entry.1.push(edge.clone());
                        }

                        if target_in {
                            let entry = subgraph
                                .entry(edge.target_id.clone())
                                .or_insert_with(|| {
                                    let node = self
                                        .global_nodes
                                        .get(&edge.target_id)
                                        .cloned()
                                        .unwrap_or_else(|| {
                                            MemoryNode::with_id(
                                                &edge.target_id,
                                                "未知节点",
                                                MemoryNodeType::Inferred,
                                            )
                                        });
                                    (node, Vec::new())
                                });
                            entry.1.push(edge.clone());
                        }
                    }
                }
            }
        }

        // 为没有边的孤立节点也创建条目
        for node_id in node_ids {
            if !subgraph.contains_key(node_id) {
                if let Some(node) = self.global_nodes.get(node_id) {
                    subgraph.insert(node_id.clone(), (node.clone(), Vec::new()));
                }
            }
        }

        subgraph
    }

    /// 获取指定图谱中的所有节点
    pub fn get_all_nodes(&self, graph_type: GraphType) -> Vec<&MemoryNode> {
        if let Some(layer) = self.layers.get(&graph_type) {
            layer.nodes.values().collect()
        } else {
            Vec::new()
        }
    }

    /// 获取全局所有节点
    pub fn get_all_global_nodes(&self) -> Vec<&MemoryNode> {
        self.global_nodes.values().collect()
    }

    /// 获取指定图谱中的所有边
    pub fn get_all_edges(&self, graph_type: GraphType) -> Vec<GraphEdge> {
        if let Some(layer) = self.layers.get(&graph_type) {
            layer.all_edges().into_iter().cloned().collect()
        } else {
            Vec::new()
        }
    }

    /// 获取所有图谱中的所有边
    pub fn get_all_edges_all_graphs(&self) -> HashMap<GraphType, Vec<GraphEdge>> {
        let mut result = HashMap::new();
        for graph_type in GraphType::all() {
            result.insert(*graph_type, self.get_all_edges(*graph_type));
        }
        result
    }

    /// 删除指定图谱中的节点
    pub fn remove_node(&mut self, graph_type: GraphType, id: &str) -> Option<MemoryNode> {
        if let Some(layer) = self.layers.get_mut(&graph_type) {
            layer.remove_node(id)
        } else {
            None
        }
    }

    /// 从所有图谱中删除节点
    pub fn remove_node_from_all(&mut self, id: &str) {
        for graph_type in GraphType::all() {
            self.remove_node(*graph_type, id);
        }
        self.global_nodes.remove(id);
    }

    /// 获取指定图谱的统计信息
    pub fn graph_stats(&self, graph_type: GraphType) -> (usize, usize) {
        if let Some(layer) = self.layers.get(&graph_type) {
            (layer.node_count(), layer.edge_count())
        } else {
            (0, 0)
        }
    }

    /// 获取全局统计信息
    pub fn global_stats(&self) -> (usize, HashMap<GraphType, (usize, usize)>) {
        let mut graph_stats = HashMap::new();
        for graph_type in GraphType::all() {
            graph_stats.insert(*graph_type, self.graph_stats(*graph_type));
        }
        (self.global_nodes.len(), graph_stats)
    }

    /// 检查节点是否存在于指定图谱中
    pub fn node_exists(&self, id: &str, graph_type: GraphType) -> bool {
        if let Some(layer) = self.layers.get(&graph_type) {
            layer.nodes.contains_key(id)
        } else {
            false
        }
    }

    /// 检查节点是否存在于全局索引中
    pub fn node_exists_global(&self, id: &str) -> bool {
        self.global_nodes.contains_key(id)
    }

    /// 获取指定图谱中两个节点之间的边
    pub fn get_edge(
        &self,
        source_id: &str,
        target_id: &str,
        graph_type: GraphType,
    ) -> Option<&GraphEdge> {
        if let Some(layer) = self.layers.get(&graph_type) {
            layer
                .get_outgoing_edges(source_id)
                .iter()
                .find(|e| e.target_id == target_id)
        } else {
            None
        }
    }

    /// 检查两个节点之间是否存在指定类型的边
    pub fn has_edge_type(
        &self,
        source_id: &str,
        target_id: &str,
        graph_type: GraphType,
        edge_type: &EdgeType,
    ) -> bool {
        if let Some(layer) = self.layers.get(&graph_type) {
            layer
                .get_outgoing_edges(source_id)
                .iter()
                .any(|e| e.target_id == target_id && &e.edge_type == edge_type)
        } else {
            false
        }
    }

    /// 获取节点的所有出边（指定图谱）
    pub fn get_outgoing_edges(&self, id: &str, graph_type: GraphType) -> Vec<&GraphEdge> {
        if let Some(layer) = self.layers.get(&graph_type) {
            layer.get_outgoing_edges(id).iter().collect()
        } else {
            Vec::new()
        }
    }

    /// 多跳推理 - 在指定图谱中执行深度遍历，发现潜在的多跳关系
    pub fn multi_hop_reasoning(
        &self,
        start_id: &str,
        graph_type: GraphType,
        max_hops: usize,
        min_confidence: f64,
    ) -> Vec<TraversalPath> {
        let all_paths = self.traverse(start_id, graph_type, max_hops);

        // 过滤低置信度路径
        all_paths
            .into_iter()
            .filter(|p| p.score >= min_confidence)
            .collect()
    }
}

impl Default for InMemoryGraphDB {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_node_and_edge() {
        let mut db = InMemoryGraphDB::new();
        let node = MemoryNode::new("测试节点", MemoryNodeType::Observation);
        let id = node.id.clone();

        db.add_node(GraphType::Semantic, node);
        assert!(db.get_node(&id).is_some());
        assert!(db.node_exists(&id, GraphType::Semantic));
        assert!(!db.node_exists(&id, GraphType::Temporal));

        let node2 = MemoryNode::new("相关节点", MemoryNodeType::Observation);
        let id2 = node2.id.clone();
        db.add_node(GraphType::Semantic, node2);

        let edge = GraphEdge::new(&id, &id2, EdgeType::SemanticSimilarity, 0.8);
        db.add_edge(GraphType::Semantic, edge);

        let neighbors = db.get_neighbors(&id, GraphType::Semantic);
        assert_eq!(neighbors.len(), 1);
    }

    #[test]
    fn test_traverse() {
        let mut db = InMemoryGraphDB::new();

        let n1 = MemoryNode::with_id("n1", "节点1", MemoryNodeType::Observation);
        let n2 = MemoryNode::with_id("n2", "节点2", MemoryNodeType::Observation);
        let n3 = MemoryNode::with_id("n3", "节点3", MemoryNodeType::Observation);

        db.add_node(GraphType::Semantic, n1);
        db.add_node(GraphType::Semantic, n2);
        db.add_node(GraphType::Semantic, n3);

        db.add_edge(
            GraphType::Semantic,
            GraphEdge::new("n1", "n2", EdgeType::SemanticSimilarity, 0.9),
        );
        db.add_edge(
            GraphType::Semantic,
            GraphEdge::new("n2", "n3", EdgeType::SemanticSimilarity, 0.7),
        );

        let paths = db.traverse("n1", GraphType::Semantic, 3);
        assert!(!paths.is_empty());

        // 应该有 n1->n2 和 n1->n2->n3 两条路径
        assert!(paths.iter().any(|p| p.nodes == vec!["n1", "n2"]));
        assert!(paths.iter().any(|p| p.nodes == vec!["n1", "n2", "n3"]));
    }

    #[test]
    fn test_multi_graph_traverse() {
        let mut db = InMemoryGraphDB::new();

        let n1 = MemoryNode::with_id("n1", "事件1", MemoryNodeType::Observation);
        let n2 = MemoryNode::with_id("n2", "事件2", MemoryNodeType::Action);
        let n3 = MemoryNode::with_id("n3", "事件3", MemoryNodeType::StateChange);

        db.add_node(GraphType::Temporal, n1.clone());
        db.add_node(GraphType::Temporal, n2);
        db.add_node(GraphType::Causal, n1);
        db.add_node(GraphType::Causal, n3);

        db.add_edge(
            GraphType::Temporal,
            GraphEdge::new("n1", "n2", EdgeType::TemporalBefore, 0.9),
        );
        db.add_edge(
            GraphType::Causal,
            GraphEdge::new("n1", "n3", EdgeType::Causal, 0.8),
        );

        let paths = db.multi_graph_traverse("n1", QueryIntent::Causal, 3);
        // 因果意图优先查因果图，然后时间图
        assert!(!paths.is_empty());
    }

    #[test]
    fn test_subgraph_extraction() {
        let mut db = InMemoryGraphDB::new();

        let n1 = MemoryNode::with_id("n1", "节点1", MemoryNodeType::Observation);
        let n2 = MemoryNode::with_id("n2", "节点2", MemoryNodeType::Observation);
        let n3 = MemoryNode::with_id("n3", "节点3", MemoryNodeType::Observation);

        db.add_node(GraphType::Semantic, n1);
        db.add_node(GraphType::Semantic, n2);
        db.add_node(GraphType::Semantic, n3);

        db.add_edge(
            GraphType::Semantic,
            GraphEdge::new("n1", "n2", EdgeType::SemanticSimilarity, 0.8),
        );
        db.add_edge(
            GraphType::Semantic,
            GraphEdge::new("n2", "n3", EdgeType::SemanticSimilarity, 0.6),
        );

        let subgraph = db.find_subgraph(&["n1".to_string(), "n2".to_string()]);
        assert!(subgraph.contains_key("n1"));
        assert!(subgraph.contains_key("n2"));
        assert!(!subgraph.contains_key("n3"));
    }
}
