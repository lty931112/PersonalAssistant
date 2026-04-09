// MAGMA 多图谱智能体记忆架构 - 向量存储实现
// 本模块实现了内存向量存储 InMemoryVectorStore，支持余弦相似度搜索和关键词匹配。

use std::collections::HashMap;

use tracing::debug;

/// 向量存储条目 - 包含向量、元数据和关键词索引
#[derive(Debug, Clone)]
struct VectorEntry {
    /// 密集向量表示
    embedding: Vec<f64>,
    /// 关联的元数据
    metadata: HashMap<String, serde_json::Value>,
    /// 从内容中提取的关键词（用于关键词搜索）
    keywords: Vec<String>,
}

/// 内存向量存储 - MAGMA 的语义检索组件
/// 支持基于余弦相似度的向量搜索和基于关键词的文本匹配
#[derive(Debug, Clone)]
pub struct InMemoryVectorStore {
    /// 向量条目存储：ID -> 条目
    entries: HashMap<String, VectorEntry>,
    /// 关键词倒排索引：关键词 -> [条目ID]
    keyword_index: HashMap<String, Vec<String>>,
}

impl InMemoryVectorStore {
    /// 创建新的内存向量存储
    pub fn new() -> Self {
        debug!("初始化 MAGMA 内存向量存储");
        Self {
            entries: HashMap::new(),
            keyword_index: HashMap::new(),
        }
    }

    /// 添加向量到存储中
    /// 如果 ID 已存在，则更新向量
    pub fn add(
        &mut self,
        id: String,
        embedding: Vec<f64>,
        metadata: HashMap<String, serde_json::Value>,
    ) {
        debug!("添加向量到存储: {} (维度: {})", id, embedding.len());

        // 从元数据中提取内容用于关键词索引
        let content = metadata
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let keywords = Self::extract_keywords(&content);

        // 如果 ID 已存在，先从关键词索引中移除旧条目
        if let Some(old_entry) = self.entries.get(&id) {
            for kw in &old_entry.keywords {
                if let Some(ids) = self.keyword_index.get_mut(kw) {
                    ids.retain(|x| x != &id);
                    if ids.is_empty() {
                        self.keyword_index.remove(kw);
                    }
                }
            }
        }

        // 更新关键词倒排索引
        for kw in &keywords {
            self.keyword_index
                .entry(kw.clone())
                .or_default()
                .push(id.clone());
        }

        // 存储向量条目
        self.entries.insert(
            id,
            VectorEntry {
                embedding,
                metadata,
                keywords,
            },
        );
    }

    /// 基于余弦相似度的向量搜索
    /// 返回最相似的 top_k 个 (ID, 相似度分数) 对
    pub fn search(&self, query: Vec<f64>, top_k: usize) -> Vec<(String, f64)> {
        if self.entries.is_empty() {
            return Vec::new();
        }

        let mut results: Vec<(String, f64)> = self
            .entries
            .iter()
            .map(|(id, entry)| {
                let similarity = cosine_similarity(&query, &entry.embedding);
                (id.clone(), similarity)
            })
            .filter(|(_, sim)| *sim > 0.0)
            .collect();

        // 按相似度降序排序
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // 取 top_k
        results.truncate(top_k);

        debug!(
            "向量搜索完成，返回 {} 个结果（共 {} 个条目）",
            results.len(),
            self.entries.len()
        );

        results
    }

    /// 带阈值的余弦相似度搜索
    /// 只返回相似度 >= threshold 的结果
    pub fn search_with_threshold(
        &self,
        query: Vec<f64>,
        top_k: usize,
        threshold: f64,
    ) -> Vec<(String, f64)> {
        let results = self.search(query, top_k);
        results
            .into_iter()
            .filter(|(_, sim)| *sim >= threshold)
            .collect()
    }

    /// 关键词搜索 - 基于倒排索引的快速文本匹配
    /// 返回匹配度最高的 top_k 个 (ID, 匹配分数) 对
    pub fn keyword_search(&self, keywords: &[String], top_k: usize) -> Vec<(String, f64)> {
        if keywords.is_empty() || self.keyword_index.is_empty() {
            return Vec::new();
        }

        // 统计每个条目的关键词命中数
        let mut hit_counts: HashMap<String, usize> = HashMap::new();

        for keyword in keywords {
            let lower_keyword = keyword.to_lowercase();
            // 精确匹配
            if let Some(ids) = self.keyword_index.get(&lower_keyword) {
                for id in ids {
                    *hit_counts.entry(id.clone()).or_insert(0) += 1;
                }
            }

            // 前缀匹配
            for (indexed_kw, ids) in &self.keyword_index {
                if indexed_kw.starts_with(&lower_keyword) || lower_keyword.starts_with(indexed_kw) {
                    for id in ids {
                        *hit_counts.entry(id.clone()).or_insert(0) += 1;
                    }
                }
            }
        }

        // 转换为分数并排序
        let total_keywords = keywords.len().max(1);
        let mut results: Vec<(String, f64)> = hit_counts
            .into_iter()
            .map(|(id, count)| (id, count as f64 / total_keywords as f64))
            .collect();

        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(top_k);

        debug!(
            "关键词搜索完成，返回 {} 个结果（查询 {} 个关键词）",
            results.len(),
            keywords.len()
        );

        results
    }

    /// 混合搜索 - 结合向量搜索和关键词搜索的结果
    /// 使用 Reciprocal Rank Fusion (RRF) 融合两种搜索结果
    pub fn hybrid_search(
        &self,
        query: Vec<f64>,
        keywords: &[String],
        top_k: usize,
        vector_weight: f64,
        keyword_weight: f64,
    ) -> Vec<(String, f64)> {
        let vector_results = self.search(query, top_k * 2);
        let keyword_results = self.keyword_search(keywords, top_k * 2);

        // RRF 融合
        let mut fused_scores: HashMap<String, f64> = HashMap::new();
        let k = 60.0; // RRF 常数

        // 向量搜索结果的 RRF 分数
        for (rank, (id, _sim)) in vector_results.iter().enumerate() {
            let rrf_score = vector_weight / (k + (rank + 1) as f64);
            *fused_scores.entry(id.clone()).or_insert(0.0) += rrf_score;
        }

        // 关键词搜索结果的 RRF 分数
        for (rank, (id, _score)) in keyword_results.iter().enumerate() {
            let rrf_score = keyword_weight / (k + (rank + 1) as f64);
            *fused_scores.entry(id.clone()).or_insert(0.0) += rrf_score;
        }

        // 排序并返回 top_k
        let mut results: Vec<(String, f64)> = fused_scores.into_iter().collect();
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(top_k);

        debug!(
            "混合搜索完成，返回 {} 个结果（向量: {}, 关键词: {}）",
            results.len(),
            vector_results.len(),
            keyword_results.len()
        );

        results
    }

    /// 获取指定 ID 的向量
    pub fn get(&self, id: &str) -> Option<&Vec<f64>> {
        self.entries.get(id).map(|e| &e.embedding)
    }

    /// 获取指定 ID 的元数据
    pub fn get_metadata(&self, id: &str) -> Option<&HashMap<String, serde_json::Value>> {
        self.entries.get(id).map(|e| &e.metadata)
    }

    /// 检查是否存在指定 ID 的向量
    pub fn contains(&self, id: &str) -> bool {
        self.entries.contains_key(id)
    }

    /// 删除指定 ID 的向量
    pub fn remove(&mut self, id: &str) -> bool {
        if let Some(entry) = self.entries.remove(id) {
            // 从关键词索引中移除
            for kw in &entry.keywords {
                if let Some(ids) = self.keyword_index.get_mut(kw) {
                    ids.retain(|x| x != id);
                    if ids.is_empty() {
                        self.keyword_index.remove(kw);
                    }
                }
            }
            true
        } else {
            false
        }
    }

    /// 获取存储中的向量数量
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 检查存储是否为空
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 获取所有条目 ID
    pub fn all_ids(&self) -> Vec<String> {
        self.entries.keys().cloned().collect()
    }

    /// 计算两个向量之间的余弦相似度
    pub fn similarity_between(&self, id_a: &str, id_b: &str) -> Option<f64> {
        let emb_a = self.entries.get(id_a)?;
        let emb_b = self.entries.get(id_b)?;
        Some(cosine_similarity(&emb_a.embedding, &emb_b.embedding))
    }

    /// 从文本中提取关键词
    /// 简单实现：按空格和标点分割，转小写，过滤停用词和短词
    fn extract_keywords(text: &str) -> Vec<String> {
        // 中文和英文的简单分词
        let mut keywords = Vec::new();

        // 按空格和常见标点分割
        for token in text.split(|c: char| c.is_whitespace() || ",.!?;:，。！？；：、（）()[]【】\"'".contains(c)) {
            let trimmed = token.trim().to_lowercase();
            if trimmed.is_empty() {
                continue;
            }

            // 英文单词：过滤停用词和短词
            if trimmed.chars().all(|c| c.is_ascii()) {
                if trimmed.len() < 2 {
                    continue;
                }
                // 简单的英文停用词过滤
                if matches!(
                    trimmed.as_str(),
                    "the" | "a" | "an" | "is" | "are" | "was" | "were"
                        | "be" | "been" | "being" | "have" | "has" | "had"
                        | "do" | "does" | "did" | "will" | "would" | "could"
                        | "should" | "may" | "might" | "can" | "shall"
                        | "to" | "of" | "in" | "for" | "on" | "with" | "at"
                        | "by" | "from" | "as" | "into" | "through" | "during"
                        | "before" | "after" | "above" | "below" | "between"
                        | "and" | "but" | "or" | "nor" | "not" | "so" | "yet"
                        | "both" | "either" | "neither" | "each" | "every"
                        | "it" | "its" | "this" | "that" | "these" | "those"
                        | "i" | "me" | "my" | "we" | "our" | "you" | "your"
                        | "he" | "him" | "his" | "she" | "her" | "they" | "them"
                ) {
                    continue;
                }
                keywords.push(trimmed);
            } else {
                // 中文：按字符分割（简单实现，实际应使用分词器）
                // 对于中文文本，将每个非标点字符作为一个关键词
                for ch in trimmed.chars() {
                    if !ch.is_ascii_punctuation() && !ch.is_whitespace() {
                        keywords.push(ch.to_string());
                    }
                }
            }
        }

        // 去重
        keywords.sort();
        keywords.dedup();
        keywords
    }
}

impl Default for InMemoryVectorStore {
    fn default() -> Self {
        Self::new()
    }
}

/// 计算两个向量之间的余弦相似度
/// 返回值范围 [-1.0, 1.0]，1.0 表示完全相同
pub fn cosine_similarity(a: &[f64], b: &[f64]) -> f64 {
    if a.is_empty() || b.is_empty() || a.len() != b.len() {
        return 0.0;
    }

    let dot_product: f64 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f64 = a.iter().map(|x| x * x).sum::<f64>().sqrt();
    let norm_b: f64 = b.iter().map(|x| x * x).sum::<f64>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    (dot_product / (norm_a * norm_b)).clamp(-1.0, 1.0)
}

/// 计算两个向量之间的欧氏距离
pub fn euclidean_distance(a: &[f64], b: &[f64]) -> f64 {
    if a.len() != b.len() {
        return f64::MAX;
    }

    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y).powi(2))
        .sum::<f64>()
        .sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_and_search() {
        let mut store = InMemoryVectorStore::new();

        let mut meta1 = HashMap::new();
        meta1.insert("content".to_string(), serde_json::Value::String("hello world".to_string()));
        store.add("doc1".to_string(), vec![1.0, 0.0, 0.0], meta1);

        let mut meta2 = HashMap::new();
        meta2.insert("content".to_string(), serde_json::Value::String("hello rust".to_string()));
        store.add("doc2".to_string(), vec![0.9, 0.1, 0.0], meta2);

        let mut meta3 = HashMap::new();
        meta3.insert("content".to_string(), serde_json::Value::String("goodbye world".to_string()));
        store.add("doc3".to_string(), vec![0.0, 0.0, 1.0], meta3);

        // 搜索与 doc1 相似的向量
        let results = store.search(vec![1.0, 0.0, 0.0], 2);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, "doc1"); // 最相似的应该是自己
        assert!(results[0].1 > 0.99);
    }

    #[test]
    fn test_keyword_search() {
        let mut store = InMemoryVectorStore::new();

        let mut meta1 = HashMap::new();
        meta1.insert("content".to_string(), serde_json::Value::String("rust programming language".to_string()));
        store.add("doc1".to_string(), vec![1.0, 0.0], meta1);

        let mut meta2 = HashMap::new();
        meta2.insert("content".to_string(), serde_json::Value::String("python programming language".to_string()));
        store.add("doc2".to_string(), vec![0.0, 1.0], meta2);

        let mut meta3 = HashMap::new();
        meta3.insert("content".to_string(), serde_json::Value::String("rust is great".to_string()));
        store.add("doc3".to_string(), vec![0.5, 0.5], meta3);

        let results = store.keyword_search(&["rust".to_string()], 5);
        assert_eq!(results.len(), 2); // doc1 和 doc3 包含 "rust"
    }

    #[test]
    fn test_hybrid_search() {
        let mut store = InMemoryVectorStore::new();

        let mut meta1 = HashMap::new();
        meta1.insert("content".to_string(), serde_json::Value::String("machine learning".to_string()));
        store.add("doc1".to_string(), vec![1.0, 0.0, 0.0], meta1);

        let mut meta2 = HashMap::new();
        meta2.insert("content".to_string(), serde_json::Value::String("deep learning neural network".to_string()));
        store.add("doc2".to_string(), vec![0.9, 0.1, 0.0], meta2);

        let results = store.hybrid_search(
            vec![1.0, 0.0, 0.0],
            &["learning".to_string()],
            5,
            1.0,
            1.0,
        );

        assert!(!results.is_empty());
    }

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 1e-10);

        let c = vec![0.0, 1.0, 0.0];
        assert!((cosine_similarity(&a, &c)).abs() < 1e-10);

        let d = vec![0.707, 0.707, 0.0];
        let sim = cosine_similarity(&a, &d);
        assert!((sim - 0.707).abs() < 0.01);
    }

    #[test]
    fn test_remove() {
        let mut store = InMemoryVectorStore::new();

        let mut meta = HashMap::new();
        meta.insert("content".to_string(), serde_json::Value::String("test content".to_string()));
        store.add("doc1".to_string(), vec![1.0, 0.0], meta);

        assert!(store.contains("doc1"));
        assert!(store.remove("doc1"));
        assert!(!store.contains("doc1"));
        assert!(!store.remove("doc1")); // 再次删除应返回 false
    }
}
