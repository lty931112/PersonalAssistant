//! WebSocket 服务器与 HTTP REST API

use std::net::SocketAddr;
use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::RwLock;
use axum::{
    Router,
    routing::{get, post},
    extract::{
        ws::{WebSocket, WebSocketUpgrade, Message as WsMessage},
        Path, State,
    },
    response::{Response, IntoResponse, Json},
    http::StatusCode,
};
use tower_http::cors::CorsLayer;
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use pa_core::CoreError;
use pa_config::Settings;
use pa_task::TaskManager;
use pa_agent::Agent;
use crate::client::ClientRegistry;
use crate::events::EventBus;
use crate::protocol::{MethodCall, MethodResponse};
use crate::metrics::MetricsCollector;

/// 应用共享状态
#[derive(Clone)]
pub struct AppState {
    /// 客户端注册表
    pub clients: Arc<RwLock<ClientRegistry>>,
    /// 事件总线
    pub event_bus: Arc<EventBus>,
    /// 配置
    pub settings: Settings,
    /// 任务管理器
    pub task_manager: Arc<TaskManager>,
    /// Agent 实例映射表
    pub agents_map: Arc<RwLock<HashMap<String, Arc<RwLock<Agent>>>>>,
    /// 指标收集器
    pub metrics: Arc<MetricsCollector>,
}

/// Gateway 服务器
pub struct GatewayServer {
    addr: SocketAddr,
    state: AppState,
}

impl GatewayServer {
    pub fn new(
        addr: impl Into<String>,
        clients: Arc<RwLock<ClientRegistry>>,
        event_bus: Arc<EventBus>,
        settings: Settings,
        task_manager: Arc<TaskManager>,
        agents_map: Arc<RwLock<HashMap<String, Arc<RwLock<Agent>>>>>,
    ) -> Self {
        Self {
            addr: addr.into().parse().expect("Invalid address"),
            state: AppState {
                clients,
                event_bus,
                settings,
                task_manager,
                agents_map,
                metrics: Arc::new(MetricsCollector::new()),
            },
        }
    }

    pub async fn run(&mut self) -> Result<(), CoreError> {
        let state = self.state.clone();

        let app = Router::new()
            // WebSocket 端点
            .route("/ws", get(ws_handler))
            // 健康检查
            .route("/health", get(health_handler))
            // Prometheus 指标
            .route("/metrics", get(metrics_handler))
            // 任务管理 API
            .route("/api/tasks", get(list_tasks_handler))
            .route("/api/tasks/:id", get(get_task_handler))
            .route("/api/tasks/:id/pause", post(pause_task_handler))
            .route("/api/tasks/:id/resume", post(resume_task_handler))
            .route("/api/tasks/:id/cancel", post(cancel_task_handler))
            // Agent 管理 API
            .route("/api/agents", get(list_agents_handler))
            .route("/api/agents/:id/status", get(get_agent_status_handler))
            .layer(CorsLayer::permissive())
            .with_state(state);

        let listener = tokio::net::TcpListener::bind(&self.addr)
            .await
            .map_err(|e| CoreError::io_error(e.to_string()))?;

        tracing::info!("WebSocket 端点: ws://{}/ws", self.addr);
        tracing::info!("HTTP REST API: http://{}/api", self.addr);
        tracing::info!("健康检查: http://{}/health", self.addr);

        axum::serve(listener, app)
            .await
            .map_err(|e| CoreError::io_error(e.to_string()))?;

        Ok(())
    }
}

// ============================================================================
// HTTP REST API 处理函数
// ============================================================================

/// 深度健康检查
///
/// 检查以下组件状态：
/// - 系统基本信息（版本、运行时间）
/// - SQLite 数据库连接
/// - Agent 状态
/// - WebSocket 客户端连接数
async fn health_handler(State(state): State<AppState>) -> impl IntoResponse {
    let start_time = std::time::Instant::now();

    // 检查 Agent 状态
    let agents = state.agents_map.read().await;
    let mut agent_health = Vec::new();
    let mut all_healthy = true;

    for (id, agent) in agents.iter() {
        let status = agent.read().await.get_status().await;
        let is_healthy = status.state == "idle" || status.state == "running";
        if !is_healthy {
            all_healthy = false;
        }
        agent_health.push(serde_json::json!({
            "id": id,
            "state": status.state,
            "healthy": is_healthy,
            "completed_tasks": status.completed_tasks,
            "total_tokens": status.total_tokens,
        }));
    }
    drop(agents);

    // 检查客户端连接数
    let client_count = state.clients.read().await.count();

    // 检查任务管理器
    let running_tasks = state.task_manager.list_running_tasks().await;
    let running_task_count = running_tasks.len();

    // 检查数据库
    let db_healthy = match state.task_manager.store().check_health().await {
        Ok(()) => true,
        Err(e) => {
            tracing::warn!("数据库健康检查失败: {}", e);
            false
        }
    };

    if !db_healthy {
        all_healthy = false;
    }

    let elapsed = start_time.elapsed();

    let status_code = if all_healthy { StatusCode::OK } else { StatusCode::SERVICE_UNAVAILABLE };
    let status_str = if all_healthy { "ok" } else { "degraded" };

    (
        status_code,
        Json(json!({
            "status": status_str,
            "version": env!("CARGO_PKG_VERSION"),
            "uptime_seconds": elapsed.as_secs_f64(),
            "components": {
                "database": { "healthy": db_healthy },
                "agents": {
                    "healthy": all_healthy,
                    "count": agent_health.len(),
                    "details": agent_health,
                },
                "clients": {
                    "connected": client_count,
                },
                "tasks": {
                    "running": running_task_count,
                },
            },
        })),
    )
}

/// Prometheus 指标端点
async fn metrics_handler(State(state): State<AppState>) -> String {
    state.metrics.render_prometheus().await
}

/// 列出所有任务
async fn list_tasks_handler(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let tasks = state.task_manager.list_all_tasks().await;
    Json(json!({
        "tasks": tasks,
        "count": tasks.len(),
    }))
}

/// 获取任务详情
async fn get_task_handler(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
) -> impl IntoResponse {
    match state.task_manager.get_task(&task_id).await {
        Ok(task) => {
            let events = state.task_manager.get_task_events(&task_id).await;
            (
                StatusCode::OK,
                Json(json!({
                    "task": task,
                    "events": events,
                })),
            )
        }
        Err(e) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": e.to_string()})),
        ),
    }
}

/// 暂停任务
async fn pause_task_handler(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
) -> impl IntoResponse {
    // 获取任务信息以构建快照
    match state.task_manager.get_task(&task_id).await {
        Ok(task_info) => {
            use pa_task::TaskSnapshot;
            let snapshot = TaskSnapshot::new(
                task_info,
                "[]",
                "",
                "",
                "{}",
            );
            match state.task_manager.pause_task(&task_id, &snapshot).await {
                Ok(()) => (StatusCode::OK, Json(json!({"status": "paused", "task_id": task_id}))),
                Err(e) => (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": e.to_string()})),
                ),
            }
        }
        Err(e) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": e.to_string()})),
        ),
    }
}

/// 恢复任务
async fn resume_task_handler(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
) -> impl IntoResponse {
    match state.task_manager.resume_task(&task_id).await {
        Ok(_snapshot) => (StatusCode::OK, Json(json!({"status": "resumed", "task_id": task_id}))),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": e.to_string()})),
        ),
    }
}

/// 取消任务
async fn cancel_task_handler(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
) -> impl IntoResponse {
    match state.task_manager.cancel_task(&task_id).await {
        Ok(()) => (StatusCode::OK, Json(json!({"status": "cancelled", "task_id": task_id}))),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": e.to_string()})),
        ),
    }
}

/// 列出所有 Agent
async fn list_agents_handler(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let agents = state.agents_map.read().await;
    let mut agent_list = Vec::new();
    for (id, agent) in agents.iter() {
        let status = agent.read().await.get_status().await;
        agent_list.push(status);
    }
    Json(json!({
        "agents": agent_list,
        "count": agent_list.len(),
    }))
}

/// 获取 Agent 状态
async fn get_agent_status_handler(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
) -> impl IntoResponse {
    let agents = state.agents_map.read().await;
    match agents.get(&agent_id) {
        Some(agent) => {
            let status = agent.read().await.get_status().await;
            (StatusCode::OK, Json(json!(status)))
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": format!("Agent {} 不存在", agent_id)})),
        ),
    }
}

// ============================================================================
// WebSocket 处理
// ============================================================================

/// WebSocket 升级处理
async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> Response {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

/// 处理 WebSocket 连接
async fn handle_socket(
    socket: WebSocket,
    state: AppState,
) {
    let client_id = uuid::Uuid::new_v4().to_string();
    tracing::info!("客户端连接: {}", client_id);

    // 注册客户端
    state.clients.write().await.register(&client_id);
    state.metrics.inc_connections();

    // 发布客户端连接事件
    state.event_bus.publish(pa_core::GatewayEvent::ClientConnected {
        client_id: client_id.clone(),
        timestamp: chrono::Utc::now(),
    });

    // 处理消息
    let (mut sender, mut receiver) = socket.split();

    while let Some(msg) = receiver.next().await {
        match msg {
            Ok(WsMessage::Text(text)) => {
                tracing::debug!("收到消息: {}", text);

                // 尝试解析为 JSON 协议消息
                let response = match serde_json::from_str::<MethodCall>(&text) {
                    Ok(method_call) => {
                        // 处理方法调用
                        handle_method_call(method_call, &state).await
                    }
                    Err(_) => {
                        // 非 JSON 协议消息，回显处理（保持向后兼容）
                        let response = MethodResponse::success(
                            &client_id,
                            json!({"echo": text}),
                        );
                        serde_json::to_string(&response).unwrap_or_else(|_| {
                            json!({"error": "序列化失败"}).to_string()
                        })
                    }
                };

                // 发送响应
                if sender.send(WsMessage::Text(response)).await.is_err() {
                    break;
                }
            }
            Ok(WsMessage::Close(_)) => {
                tracing::info!("客户端断开: {}", client_id);
                break;
            }
            _ => {}
        }
    }

    // 移除客户端
    state.clients.write().await.unregister(&client_id);
    state.metrics.dec_connections();

    // 发布客户端断开事件
    state.event_bus.publish(pa_core::GatewayEvent::ClientDisconnected {
        client_id: client_id.clone(),
        reason: "normal".to_string(),
    });
}

/// 处理 JSON 协议方法调用
///
/// 支持的方法：
/// - "query": 发送查询到 Agent
/// - "cancel": 取消当前任务
/// - "status": 查询任务状态
async fn handle_method_call(call: MethodCall, state: &AppState) -> String {
    let call_id = call.id.clone();
    let method = call.method.clone();
    let params = call.params.clone();

    tracing::info!("方法调用: {} (id={})", method, call_id);

    let response = match method.as_str() {
        // 查询方法：发送查询到 Agent
        "query" => {
            let prompt = params.get("prompt")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let agent_id = params.get("agent_id")
                .and_then(|v| v.as_str())
                .unwrap_or("default");

            // 发布请求路由事件
            state.event_bus.publish(pa_core::GatewayEvent::RequestRouted {
                request_id: call_id.clone(),
                agent_id: agent_id.to_string(),
                strategy: "direct".to_string(),
            });

            // 查找目标 Agent
            let agents = state.agents_map.read().await;
            match agents.get(agent_id) {
                Some(agent_arc) => {
                    let mut agent = agent_arc.write().await;
                    let config = pa_query::QueryConfig::default();
                    let result = agent.query_with_task(prompt.to_string(), config).await;
                    state.metrics.inc_requests();
                    MethodResponse::success(&call_id, json!({
                        "result": result,
                        "agent_id": agent_id,
                    }))
                }
                None => {
                    MethodResponse::error(&call_id, format!("Agent {} 不存在", agent_id))
                }
            }
        }

        // 取消方法：取消当前任务
        "cancel" => {
            let task_id = params.get("task_id")
                .and_then(|v| v.as_str());

            if let Some(tid) = task_id {
                match state.task_manager.cancel_task(tid).await {
                    Ok(()) => {
                        MethodResponse::success(&call_id, json!({
                            "status": "cancelled",
                            "task_id": tid,
                        }))
                    }
                    Err(e) => {
                        MethodResponse::error(&call_id, format!("取消任务失败: {}", e))
                    }
                }
            } else {
                // 尝试取消所有运行中的任务
                let running = state.task_manager.list_running_tasks().await;
                let mut cancelled_count = 0u32;
                for task in &running {
                    if state.task_manager.cancel_task(&task.id).await.is_ok() {
                        cancelled_count += 1;
                    }
                }
                MethodResponse::success(&call_id, json!({
                    "status": "cancelled",
                    "cancelled_count": cancelled_count,
                }))
            }
        }

        // 状态方法：查询任务状态
        "status" => {
            let task_id = params.get("task_id")
                .and_then(|v| v.as_str());

            if let Some(tid) = task_id {
                match state.task_manager.get_task(tid).await {
                    Ok(task) => {
                        let events = state.task_manager.get_task_events(tid).await;
                        MethodResponse::success(&call_id, json!({
                            "task": task,
                            "recent_events": events,
                        }))
                    }
                    Err(e) => {
                        MethodResponse::error(&call_id, format!("获取任务状态失败: {}", e))
                    }
                }
            } else {
                // 返回所有运行中任务的状态
                let running = state.task_manager.list_running_tasks().await;
                MethodResponse::success(&call_id, json!({
                    "running_tasks": running,
                    "count": running.len(),
                }))
            }
        }

        // 未知方法
        _ => {
            MethodResponse::error(&call_id, format!("未知方法: {}", method))
        }
    };

    serde_json::to_string(&response).unwrap_or_else(|_| {
        json!({"error": "序列化响应失败"}).to_string()
    })
}
