//! MAGMA - Multi-Graph based Agentic Memory Architecture
//! 多图谱智能体记忆架构
//!
//! MAGMA 是一个基于多图谱的智能体记忆系统，通过四个正交图谱（语义、时间、因果、实体）
//! 管理和检索智能体的记忆，支持快速流摄取和慢速流整合的双流记忆管理。
//!
//! # 核心概念
//!
//! - **四个正交图谱**：语义图、时间图、因果图、实体图
//! - **双流记忆管理**：快速流（立即摄取）+ 慢速流（异步整合）
//! - **自适应检索**：基于查询意图的四阶段检索管道
//! - **记忆整合**：重复检测、矛盾解决、关系推断、低频修剪

#![warn(missing_docs)]
#![warn(clippy::all)]

/// 核心类型定义
pub mod types;

/// 图数据库实现
pub mod graph;

/// 向量存储实现
pub mod vector;

/// 查询引擎实现
pub mod query;

/// 记忆引擎主入口
pub mod engine;

/// 记忆整合实现
pub mod integration;

// 重新导出常用类型
pub use types::{
    EdgeType, GraphEdge, GraphType, IntegrationReport, MemoryConfig, MemoryError, MemoryNode,
    MemoryNodeType, QueryIntent, QueryAnalysis, RetrievalResult, Result, TraversalPath,
};

pub use engine::{EngineStats, MagmaMemoryEngine};

pub use graph::InMemoryGraphDB;

pub use vector::{cosine_similarity, euclidean_distance, InMemoryVectorStore};

pub use query::{MemoryQueryEngine, QueryConfig};

pub use integration::MemoryIntegrator;
