// MAGMA 多图谱智能体记忆架构 - 查询引擎实现
// 本模块实现了 MAGMA 的自适应层次检索引擎，包含四阶段检索管道：
// 1. 查询分析与分解
// 2. 多信号锚点识别（RRF 融合）
// 3. 自适应遍历策略
// 4. 上下文合成

use std::collections::HashMap;

use tracing::{debug, info};

use crate::graph::InMemoryGraphDB;
use crate::types::*;
use crate::vector::InMemoryVectorStore;

/// 查询引擎配置
#[derive(Debug, Clone)]
pub struct QueryConfig {
    /// 初始向量搜索结果数
    pub vector_search_k: usize,
    /// 最小关键词分数
    pub keyword_threshold: f64,
    /// 最终上下文节点数
    pub top_k_final: usize,
    /// 最大遍历跳数
    pub max_traversal_hops: usize,
    /// 语义相似度阈值
    pub similarity_threshold: f64,
}

impl Default for QueryConfig {
    fn default() -> Self {
        Self {
            vector_search_k: 20,
            keyword_threshold: 0.3,
            top_k_final: 5,
            max_traversal_hops: 3,
            similarity_threshold: 0.7,
        }
    }
}

/// 查询引擎 - MAGMA 的自适应检索核心
/// 整合向量搜索和图谱遍历，实现四阶段检索管道
pub struct MemoryQueryEngine {
    /// 图数据库引用
    graph_db: InMemoryGraphDB,
    /// 向量存储引用
    vector_store: InMemoryVectorStore,
    /// 查询配置
    config: QueryConfig,
}

impl MemoryQueryEngine {
    /// 创建新的查询引擎
    pub fn new(graph_db: InMemoryGraphDB, vector_store: InMemoryVectorStore, config: QueryConfig) -> Self {
        info!("初始化 MAGMA 查询引擎");
        Self {
            graph_db,
            vector_store,
            config,
        }
    }

    /// 四阶段检索管道 - 查询记忆的主入口
    ///
    /// 阶段 1: 查询分析与分解 - 理解查询意图，提取关键词和实体
    /// 阶段 2: 多信号锚点识别 - 通过向量搜索和关键词搜索找到候选锚点
    /// 阶段 3: 自适应遍历策略 - 根据意图在图谱中遍历扩展
    /// 阶段 4: 上下文合成 - 将遍历结果合成为最终检索结果
    pub async fn retrieve(
        &self,
        query: &str,
        intent: Option<QueryIntent>,
    ) -> Result<RetrievalResult> {
        info!("开始检索: query='{}', intent={:?}", query, intent);

        // 阶段 1: 查询分析与分解
        let analysis = self.analyze_query(query);
        let resolved_intent = intent.unwrap_or(analysis.intent.clone());
        debug!("阶段1完成 - 意图: {}, 关键词: {:?}", resolved_intent, analysis.keywords);

        // 阶段 2: 多信号锚点识别（RRF 融合）
        let anchors = self.identify_anchors(&analysis);
        debug!("阶段2完成 - 识别到 {} 个锚点", anchors.len());

        if anchors.is_empty() {
            debug!("未找到锚点，返回空结果");
            return Ok(RetrievalResult::empty(resolved_intent));
        }

        // 阶段 3: 自适应遍历策略
        let max_hops = self
            .config
            .max_traversal_hops
            .min(resolved_intent.max_hops());
        let paths = self.adaptive_traverse(&anchors, resolved_intent.clone(), max_hops);
        debug!("阶段3完成 - 发现 {} 条遍历路径", paths.len());

        // 阶段 4: 上下文合成
        let result = self.synthesize_context(paths, &resolved_intent);
        debug!(
            "阶段4完成 - 最终结果: {} 个节点, 置信度: {:.3}",
            result.nodes.len(),
            result.confidence
        );

        Ok(result)
    }

    /// 阶段 1: 查询分析与分解
    ///
    /// 分析查询文本，提取以下信息：
    /// - 关键词：用于关键词搜索
    /// - 查询意图：决定检索策略
    /// - 时间引用：用于时间图谱检索
    /// - 实体引用：用于实体图谱检索
    fn analyze_query(&self, query: &str) -> QueryAnalysis {
        let keywords = self.extract_keywords(query);
        let intent = self.detect_intent(query);
        let temporal_references = self.extract_temporal_refs(query);
        let entity_references = self.extract_entity_refs(query);

        QueryAnalysis {
            raw_query: query.to_string(),
            keywords,
            intent,
            temporal_references,
            entity_references,
        }
    }

    /// 从查询文本中提取关键词
    fn extract_keywords(&self, query: &str) -> Vec<String> {
        let mut keywords = Vec::new();

        // 按空格和标点分割
        for token in query.split(|c: char| {
            c.is_whitespace() || ",.!?;:，。！？；：、（）()[]【】\"'".contains(c)
        }) {
            let trimmed = token.trim().to_lowercase();
            if trimmed.is_empty() {
                continue;
            }

            // 过滤常见停用词
            if matches!(
                trimmed.as_str(),
                "的" | "了" | "在" | "是" | "我" | "有" | "和" | "就"
                    | "不" | "人" | "都" | "一" | "一个" | "上" | "也"
                    | "很" | "到" | "说" | "要" | "去" | "你" | "会"
                    | "着" | "没有" | "看" | "好" | "自己" | "这"
                    | "the" | "a" | "an" | "is" | "are" | "was" | "were"
                    | "be" | "have" | "has" | "had" | "do" | "does" | "did"
                    | "will" | "would" | "could" | "should" | "can"
                    | "to" | "of" | "in" | "for" | "on" | "with" | "at"
                    | "by" | "from" | "and" | "but" | "or" | "not"
                    | "what" | "when" | "where" | "who" | "how" | "why"
                    | "which" | "that" | "this" | "it"
            ) {
                continue;
            }

            keywords.push(trimmed);
        }

        // 去重
        keywords.sort();
        keywords.dedup();
        keywords
    }

    /// 检测查询意图
    /// 基于查询文本中的关键词和模式推断意图
    fn detect_intent(&self, query: &str) -> QueryIntent {
        let query_lower = query.to_lowercase();

        // 时间线跟踪意图
        let temporal_patterns = [
            "什么时候",
            "时间线",
            "时间顺序",
            "先后",
            "之前",
            "之后",
            "when",
            "timeline",
            "before",
            "after",
            "sequence",
            "chronological",
            "顺序",
            "历史",
            "发展",
        ];

        // 因果分析意图
        let causal_patterns = [
            "为什么",
            "原因",
            "导致",
            "因为",
            "所以",
            "因果",
            "影响",
            "根本原因",
            "why",
            "cause",
            "reason",
            "because",
            "result",
            "impact",
            "effect",
            "consequence",
        ];

        // 事实检查意图
        let factual_patterns = [
            "是什么",
            "是谁",
            "定义",
            "属性",
            "特征",
            "什么",
            "哪个",
            "who",
            "what",
            "which",
            "define",
            "definition",
            "fact",
            "属性",
            "信息",
            "详情",
        ];

        // 计算各意图的匹配分数
        let temporal_score = temporal_patterns
            .iter()
            .filter(|p| query_lower.contains(*p))
            .count();

        let causal_score = causal_patterns
            .iter()
            .filter(|p| query_lower.contains(*p))
            .count();

        let factual_score = factual_patterns
            .iter()
            .filter(|p| query_lower.contains(*p))
            .count();

        // 选择得分最高的意图
        let max_score = temporal_score.max(causal_score).max(factual_score);

        if max_score == 0 {
            // 默认为开放域搜索
            QueryIntent::OpenDomain
        } else if temporal_score == max_score {
            QueryIntent::Temporal
        } else if causal_score == max_score {
            QueryIntent::Causal
        } else {
            QueryIntent::Factual
        }
    }

    /// 提取查询中的时间引用
    fn extract_temporal_refs(&self, query: &str) -> Vec<String> {
        let mut refs = Vec::new();

        // 简单的时间模式匹配
        let time_patterns = [
            r"\d{4}年",
            r"\d{1,2}月",
            r"\d{1,2}日",
            r"\d{4}-\d{2}-\d{2}",
            r"昨天",
            r"今天",
            r"明天",
            r"上周",
            r"下周",
            r"上个月",
            r"下个月",
            r"去年",
            r"今年",
            r"昨天",
            r"last\s+\w+",
            r"next\s+\w+",
            r"\d+\s+(days?|weeks?|months?|years?)\s+ago",
        ];

        for pattern in &time_patterns {
            // 简单的字符串包含检查（实际应使用正则表达式）
            if query.contains(pattern) || pattern.chars().all(|c| c.is_ascii()) {
                // 对于 ASCII 模式，尝试在查询中查找
                if query.to_lowercase().contains(&pattern.to_lowercase()) {
                    refs.push(pattern.to_string());
                }
            } else if query.contains(pattern) {
                refs.push(pattern.to_string());
            }
        }

        refs
    }

    /// 提取查询中的实体引用
    fn extract_entity_refs(&self, query: &str) -> Vec<String> {
        // 简单实现：提取引号中的内容和大写开头的词组
        let mut entities = Vec::new();

        // 提取引号中的内容
        let mut in_quotes = false;
        let mut current = String::new();
        for ch in query.chars() {
            match ch {
                '"' | '"' | '"' | '\'' | '\'' | '\'' => {
                    if in_quotes {
                        if !current.trim().is_empty() {
                            entities.push(current.trim().to_string());
                        }
                        current.clear();
                    }
                    in_quotes = !in_quotes;
                }
                _ if in_quotes => {
                    current.push(ch);
                }
                _ => {}
            }
        }

        // 提取大写开头的英文词组（简单实现）
        let mut words = query.split_whitespace().peekable();
        while let Some(word) = words.next() {
            if word.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) && word.len() > 1 {
                let mut entity = word.to_string();
                // 检查后续是否也是大写开头的词（连续的专有名词）
                while words.peek().map(|w| w.chars().next().map(|c| c.is_uppercase()).unwrap_or(false)).unwrap_or(false) {
                    entity.push(' ');
                    entity.push_str(words.next().unwrap());
                }
                entities.push(entity);
            }
        }

        entities.sort();
        entities.dedup();
        entities
    }

    /// 阶段 2: 多信号锚点识别
    ///
    /// 通过向量搜索和关键词搜索两种信号找到候选锚点，
    /// 使用 Reciprocal Rank Fusion (RRF) 融合排序结果
    fn identify_anchors(&self, analysis: &QueryAnalysis) -> Vec<String> {
        let mut anchor_scores: HashMap<String, f64> = HashMap::new();
        let k = 60.0; // RRF 常数

        // 信号 1: 向量搜索
        // 使用查询文本生成伪向量（在实际系统中应使用嵌入模型）
        let query_embedding = self.generate_query_embedding(&analysis.raw_query);
        if !query_embedding.is_empty() {
            let vector_results = self
                .vector_store
                .search(query_embedding, self.config.vector_search_k);

            for (rank, (id, _similarity)) in vector_results.iter().enumerate() {
                let rrf_score = 1.0 / (k + (rank + 1) as f64);
                *anchor_scores.entry(id.clone()).or_insert(0.0) += rrf_score;
            }
        }

        // 信号 2: 关键词搜索
        if !analysis.keywords.is_empty() {
            let keyword_results = self
                .vector_store
                .keyword_search(&analysis.keywords, self.config.vector_search_k);

            for (rank, (id, score)) in keyword_results.iter().enumerate() {
                if *score >= self.config.keyword_threshold {
                    let rrf_score = 1.0 / (k + (rank + 1) as f64);
                    *anchor_scores.entry(id.clone()).or_insert(0.0) += rrf_score;
                }
            }
        }

        // 信号 3: 实体引用匹配
        for entity in &analysis.entity_references {
            // 在图数据库中搜索包含实体名称的节点
            for node in self.graph_db.get_all_global_nodes() {
                if node.content.contains(entity)
                    || node
                        .attributes
                        .get("entity_name")
                        .and_then(|v| v.as_str())
                        .map(|s| s == entity.as_str())
                        .unwrap_or(false)
                {
                    // 实体精确匹配给予较高分数
                    *anchor_scores.entry(node.id.clone()).or_insert(0.0) += 0.1;
                }
            }
        }

        // 排序并返回锚点 ID
        let mut anchors: Vec<(String, f64)> = anchor_scores.into_iter().collect();
        anchors.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let result: Vec<String> = anchors.into_iter().map(|(id, _)| id).collect();
        debug!("锚点识别结果: {:?}", result);
        result
    }

    /// 生成查询的伪嵌入向量
    /// 在实际系统中应调用嵌入模型，这里使用简单的哈希方法生成
    fn generate_query_embedding(&self, query: &str) -> Vec<f64> {
        // 使用简单的字符哈希生成固定维度的伪向量
        let dimensions = 128;
        let mut embedding = vec![0.0f64; dimensions];

        for (i, ch) in query.chars().enumerate() {
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

    /// 阶段 3: 自适应遍历策略
    ///
    /// 根据查询意图选择不同的图谱遍历策略：
    /// - 事实检查：优先实体图和语义图，浅层遍历
    /// - 时间线跟踪：专注时间图，深层遍历
    /// - 因果分析：从因果图开始，多跳推理
    /// - 开放域：全图谱搜索，中等深度
    fn adaptive_traverse(
        &self,
        anchors: &[String],
        intent: QueryIntent,
        max_hops: usize,
    ) -> Vec<TraversalPath> {
        let mut all_paths = Vec::new();
        let priority_graphs = intent.priority_graphs();

        // 限制锚点数量以避免过度遍历
        let effective_anchors = anchors.iter().take(10);

        for anchor_id in effective_anchors {
            // 检查锚点是否存在于图数据库中
            if !self.graph_db.node_exists_global(anchor_id) {
                debug!("锚点 {} 不存在于图数据库中，跳过", anchor_id);
                continue;
            }

            // 根据意图选择遍历策略
            match &intent {
                QueryIntent::Factual => {
                    // 事实检查：在实体图和语义图中浅层遍历
                    for graph_type in &priority_graphs {
                        let paths = self.graph_db.traverse(anchor_id, *graph_type, max_hops.min(2));
                        all_paths.extend(paths);
                    }
                }
                QueryIntent::Temporal => {
                    // 时间线跟踪：在时间图中深层遍历
                    let paths = self.graph_db.traverse(
                        anchor_id,
                        GraphType::Temporal,
                        max_hops.max(5),
                    );
                    all_paths.extend(paths);

                    // 同时在因果图中查找相关路径
                    let causal_paths = self.graph_db.traverse(
                        anchor_id,
                        GraphType::Causal,
                        max_hops,
                    );
                    all_paths.extend(causal_paths);
                }
                QueryIntent::Causal => {
                    // 因果分析：从因果图开始多跳推理
                    let causal_paths = self
                        .graph_db
                        .multi_hop_reasoning(anchor_id, GraphType::Causal, max_hops, 0.3);
                    all_paths.extend(causal_paths);

                    // 在时间图中查找因果链的上下文
                    let temporal_paths = self.graph_db.traverse(
                        anchor_id,
                        GraphType::Temporal,
                        max_hops,
                    );
                    all_paths.extend(temporal_paths);
                }
                QueryIntent::OpenDomain => {
                    // 开放域：全图谱搜索
                    let paths = self
                        .graph_db
                        .multi_graph_traverse(anchor_id, intent.clone(), max_hops);
                    all_paths.extend(paths);
                }
            }
        }

        // 按评分排序并去重
        all_paths.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        // 去重：基于路径节点序列
        all_paths.dedup_by(|a, b| a.nodes == b.nodes);

        debug!("自适应遍历完成，共 {} 条路径", all_paths.len());
        all_paths
    }

    /// 阶段 4: 上下文合成
    ///
    /// 将遍历路径合成为最终的检索结果：
    /// 1. 从路径中提取所有唯一节点
    /// 2. 按相关度评分排序
    /// 3. 截取 top_k 个节点
    /// 4. 计算整体置信度
    fn synthesize_context(
        &self,
        paths: Vec<TraversalPath>,
        intent: &QueryIntent,
    ) -> RetrievalResult {
        if paths.is_empty() {
            return RetrievalResult::empty(intent.clone());
        }

        // 收集所有唯一节点及其评分
        let mut node_scores: HashMap<String, (MemoryNode, f64)> = HashMap::new();

        for path in &paths {
            for (i, node_id) in path.nodes.iter().enumerate() {
                // 节点在路径中的位置越靠前，评分越高
                let position_score = 1.0 / (1 + i) as f64;
                // 路径本身的评分作为权重
                let combined_score = position_score * path.score;

                if let Some(node) = self.graph_db.get_node(node_id) {
                    let entry = node_scores
                        .entry(node_id.clone())
                        .or_insert_with(|| (node.clone(), 0.0));
                    entry.1 += combined_score;
                }
            }
        }

        // 按评分排序
        let mut sorted_nodes: Vec<(MemoryNode, f64)> = node_scores.into_values().collect();
        sorted_nodes.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // 截取 top_k 个节点
        let top_k = self.config.top_k_final;
        sorted_nodes.truncate(top_k);

        // 提取节点和计算置信度
        let nodes: Vec<MemoryNode> = sorted_nodes.iter().map(|(n, _)| n.clone()).collect();
        let avg_score = if sorted_nodes.is_empty() {
            0.0
        } else {
            sorted_nodes.iter().map(|(_, s)| s).sum::<f64>() / sorted_nodes.len() as f64
        };

        // 归一化置信度
        let confidence = (avg_score * 10.0).clamp(0.0, 1.0);

        // 保留评分最高的路径
        let mut final_paths = paths;
        final_paths.truncate(10);

        RetrievalResult {
            nodes,
            paths: final_paths,
            confidence,
            intent: intent.clone(),
        }
    }

    /// 从检索结果中生成上下文文本
    /// 将节点内容格式化为可读的上下文字符串
    pub fn format_context(result: &RetrievalResult) -> String {
        if result.nodes.is_empty() {
            return String::new();
        }

        let mut context = String::new();
        context.push_str(&format!("[检索意图: {}]\n", result.intent));
        context.push_str(&format!("[置信度: {:.1}%]\n\n", result.confidence * 100.0));

        for (i, node) in result.nodes.iter().enumerate() {
            context.push_str(&format!(
                "{}. [{}] ({}) {}\n",
                i + 1,
                node.node_type,
                node.timestamp.format("%Y-%m-%d %H:%M:%S"),
                node.content
            ));

            // 添加关键属性
            for (key, value) in &node.attributes {
                if key != "access_count" && key != "embedding" {
                    context.push_str(&format!("   - {}: {}\n", key, value));
                }
            }
            context.push('\n');
        }

        // 添加路径信息
        if !result.paths.is_empty() {
            context.push_str("[关联路径]\n");
            for (i, path) in result.paths.iter().take(3).enumerate() {
                context.push_str(&format!(
                    "  路径{}: {} (评分: {:.3})\n",
                    i + 1,
                    path.nodes.join(" -> "),
                    path.score
                ));
            }
        }

        context
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyze_query() {
        let engine = create_test_engine();

        // 测试时间意图检测
        let analysis = engine.analyze_query("这件事是什么时候发生的？");
        assert_eq!(analysis.intent, QueryIntent::Temporal);

        // 测试因果意图检测
        let analysis = engine.analyze_query("为什么会导致这个问题？");
        assert_eq!(analysis.intent, QueryIntent::Causal);

        // 测试事实意图检测
        let analysis = engine.analyze_query("Rust语言是什么？");
        assert_eq!(analysis.intent, QueryIntent::Factual);
    }

    #[test]
    fn test_identify_anchors() {
        let engine = create_test_engine();

        let analysis = QueryAnalysis {
            raw_query: "test query".to_string(),
            keywords: vec!["test".to_string()],
            intent: QueryIntent::OpenDomain,
            temporal_references: Vec::new(),
            entity_references: Vec::new(),
        };

        let anchors = engine.identify_anchors(&analysis);
        // 在空存储中应该返回空列表
        assert!(anchors.is_empty());
    }

    #[tokio::test]
    async fn test_retrieve_empty() {
        let engine = create_test_engine();
        let result = engine.retrieve("test query", None).await.unwrap();
        assert!(result.nodes.is_empty());
    }

    fn create_test_engine() -> MemoryQueryEngine {
        let graph_db = InMemoryGraphDB::new();
        let vector_store = InMemoryVectorStore::new();
        let config = QueryConfig::default();
        MemoryQueryEngine::new(graph_db, vector_store, config)
    }
}
