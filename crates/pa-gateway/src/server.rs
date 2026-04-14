//! WebSocket 服务器与 HTTP REST API

use std::net::SocketAddr;
use std::sync::Arc;
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::sync::oneshot;
use axum::{
    Router,
    routing::{get, post},
    extract::{
        ws::{WebSocket, WebSocketUpgrade, Message as WsMessage},
        Path, Request, State,
    },
    middleware::{self, Next},
    response::{IntoResponse, Json, Response},
    http::{Method, StatusCode},
};
use crate::auth::{extract_gateway_credential, gateway_auth_enabled, verify_gateway_credential};
use tower_http::cors::CorsLayer;
use futures_util::{SinkExt, StreamExt as FuturesStreamExt, stream::Stream};
use axum::response::sse::{Event, KeepAlive, Sse};
use tokio_stream::{StreamExt as TokioStreamExt, wrappers::BroadcastStream};
use std::convert::Infallible;
use serde::Deserialize;
use serde_json::json;
use pa_core::CoreError;
use pa_config::{PersonaRuntime, Settings};
use pa_task::TaskManager;
use pa_query::SharedApprovalBroker;
use pa_agent::Agent;
use crate::client::ClientRegistry;
use crate::events::EventBus;
use crate::protocol::{MethodCall, MethodResponse};
use crate::metrics::MetricsCollector;
use crate::log_broadcast::LogBroadcast;

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
    /// 工具人工批准（可选）
    pub approval_broker: Option<Arc<SharedApprovalBroker>>,
    /// 「伏羲」人格与任务/流程代号
    pub persona: Arc<PersonaRuntime>,
    /// 实时日志广播（tracing 副本，SSE：`/api/logs/stream`）
    pub log_broadcast: LogBroadcast,
    /// 子流程状态（主流程负责下发与监控）
    pub subflow_states: Arc<RwLock<HashMap<String, SubflowState>>>,
    /// 子流程执行句柄
    pub subflow_handles: Arc<RwLock<HashMap<String, tokio::task::JoinHandle<()>>>>,
}

/// 查询子流程状态
#[derive(Debug, Clone, serde::Serialize)]
pub struct SubflowState {
    pub call_id: String,
    pub agent_id: String,
    pub task_id: Option<String>,
    pub state: String,
    pub message: Option<String>,
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
        approval_broker: Option<Arc<SharedApprovalBroker>>,
        persona: Arc<PersonaRuntime>,
        log_broadcast: LogBroadcast,
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
                approval_broker,
                persona,
                log_broadcast,
                subflow_states: Arc::new(RwLock::new(HashMap::new())),
                subflow_handles: Arc::new(RwLock::new(HashMap::new())),
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
            .route("/api/subflows", get(list_subflows_handler))
            .route("/api/subflows/:id/cancel", post(cancel_subflow_handler))
            // Agent 管理 API
            .route("/api/agents", get(list_agents_handler))
            .route("/api/agents/:id/status", get(get_agent_status_handler))
            .route("/api/logs/stream", get(logs_stream_handler))
            .route("/api/audit/trace/:trace_id", get(get_audit_trace_handler))
            .route("/api/approvals/pending", get(list_pending_approvals_handler))
            .route(
                "/api/approvals/:approval_id/respond",
                post(respond_approval_handler),
            )
            .layer(middleware::from_fn_with_state(
                state.clone(),
                gateway_auth_middleware,
            ))
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

/// 当 `[gateway].auth_token` 非空时，除 `OPTIONS`（CORS 预检）与 `GET /health` 外均需携带有效凭证。
///
/// 凭证：`Authorization: Bearer …`、`X-PA-Token`、或查询参数 `token=`（供浏览器 WebSocket 使用）。
async fn gateway_auth_middleware(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    if gateway_auth_skip(&request) {
        return next.run(request).await;
    }
    if !gateway_auth_enabled(&state.settings) {
        return next.run(request).await;
    }
    let uri = request.uri().clone();
    let cred = extract_gateway_credential(request.headers(), &uri);
    if !verify_gateway_credential(&state.settings, cred.as_deref()) {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }
    next.run(request).await
}

fn gateway_auth_skip(request: &Request) -> bool {
    *request.method() == Method::OPTIONS || request.uri().path() == "/health"
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
    let lock_timeout = Duration::from_millis(300);

    // 检查 Agent 状态
    let agents = state.agents_map.read().await;
    let mut agent_health = Vec::new();
    let mut all_healthy = true;

    for (id, agent) in agents.iter() {
        // 查询过程中 Agent 可能持有写锁较长时间；健康检查采用短超时，避免 /health 被阻塞。
        match tokio::time::timeout(lock_timeout, agent.read()).await {
            Ok(agent_guard) => {
                let status = agent_guard.get_status().await;
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
            Err(_) => {
                all_healthy = false;
                agent_health.push(serde_json::json!({
                    "id": id,
                    "state": "busy",
                    "healthy": false,
                    "reason": "agent_lock_timeout",
                    "completed_tasks": 0,
                    "total_tokens": 0,
                }));
            }
        }
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

/// Server-Sent Events：实时推送与 stderr 相同的 tracing 行（浏览器可用 `EventSource`，鉴权同 `token=` 查询参数）。
async fn logs_stream_handler(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.log_broadcast.subscribe();
    let stream = TokioStreamExt::filter_map(BroadcastStream::new(rx), |item| match item {
        Ok(line) => Some(Ok(Event::default().data(line))),
        Err(_) => None,
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
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

/// 列出查询子流程状态（主流程监控视角）
async fn list_subflows_handler(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let states = state.subflow_states.read().await;
    let items: Vec<SubflowState> = states.values().cloned().collect();
    Json(json!({
        "subflows": items,
        "count": items.len(),
    }))
}

/// 取消子流程（通过子流程关联的 task_id 触发取消）
async fn cancel_subflow_handler(
    State(state): State<AppState>,
    Path(subflow_id): Path<String>,
) -> impl IntoResponse {
    let maybe_task_id = {
        let states = state.subflow_states.read().await;
        states.get(&subflow_id).and_then(|s| s.task_id.clone())
    };

    // 第一阶段：子流程尚未创建 task_id（或创建前），直接中止子流程句柄
    if maybe_task_id.is_none() {
        let aborted = {
            let mut handles = state.subflow_handles.write().await;
            if let Some(handle) = handles.remove(&subflow_id) {
                handle.abort();
                true
            } else {
                false
            }
        };

        if aborted {
            let mut states = state.subflow_states.write().await;
            if let Some(s) = states.get_mut(&subflow_id) {
                s.state = "cancelled".to_string();
                s.message = Some("子流程已在分配 task_id 前中止".to_string());
            }
            return (
                StatusCode::OK,
                Json(json!({
                    "status": "cancelled",
                    "subflow_id": subflow_id,
                    "phase": "before_task_created"
                })),
            );
        }

        return (
            StatusCode::NOT_FOUND,
            Json(json!({
                "error": "subflow 不存在",
                "subflow_id": subflow_id
            })),
        );
    }

    let task_id = maybe_task_id.unwrap_or_default();

    match state.task_manager.cancel_task(&task_id).await {
        Ok(()) => {
            let mut states = state.subflow_states.write().await;
            if let Some(s) = states.get_mut(&subflow_id) {
                s.state = "cancel_requested".to_string();
                s.message = Some("已提交取消请求".to_string());
            }
            (
                StatusCode::OK,
                Json(json!({
                    "status": "cancel_requested",
                    "subflow_id": subflow_id,
                    "task_id": task_id
                })),
            )
        }
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": e.to_string(),
                "subflow_id": subflow_id,
                "task_id": task_id
            })),
        ),
    }
}

/// 列出所有 Agent
async fn list_agents_handler(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let agents = state.agents_map.read().await;
    let mut agent_list = Vec::new();
    for (_id, agent) in agents.iter() {
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
// 查询：会话层系统提示覆盖（对齐 OpenClaw 式「每会话人格」+ 可选 emoji 风格）
// ============================================================================

/// 合并 Web 端传入的 `session_system_prompt`（本会话人格）与 `use_emoji`（是否在回复中使用表情符号）。
fn apply_session_query_overrides(config: &mut pa_query::QueryConfig, params: &serde_json::Value) {
    if let Some(s) = params.get("session_system_prompt").and_then(|v| v.as_str()) {
        let s = s.trim();
        if !s.is_empty() {
            let base = config.system_prompt.trim();
            config.system_prompt = format!(
                "{}\n\n【本会话人格与行为设定】\n{}",
                base, s
            );
        }
    }
    let use_emoji = params
        .get("use_emoji")
        .and_then(|v| {
            v.as_bool().or_else(|| {
                v.as_str()
                    .map(|s| s.eq_ignore_ascii_case("true") || s == "1")
            })
        })
        .unwrap_or(false);
    if use_emoji {
        config.system_prompt.push_str(
            "\n\n【输出风格】在合适处自然使用 Unicode 表情符号（emoji）表达语气；与语境一致，避免堆砌。",
        );
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

    while let Some(msg) = FuturesStreamExt::next(&mut receiver).await {
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

            // 查找目标 Agent（主流程只做任务下发；实际执行在子流程中）
            let agent_arc = {
                let agents = state.agents_map.read().await;
                agents.get(agent_id).cloned()
            };

            let Some(agent_arc) = agent_arc else {
                MethodResponse::error(&call_id, format!("Agent {} 不存在", agent_id))
            };

            let (mut config, display_name) = {
                let agent = agent_arc.read().await;
                (agent.build_query_config(), agent.display_name().to_string())
            };
            let base_role = config.system_prompt.clone();
            config.system_prompt = state.persona.build_system_prompt(
                agent_id,
                display_name.as_str(),
                base_role.as_str(),
            );
            apply_session_query_overrides(&mut config, &params);
            let plan_codename = state.persona.next_plan_codename();
            let mythic = PersonaRuntime::stable_mythic_codename(agent_id);
            let task_meta = serde_json::json!({
                "plan_codename": plan_codename,
                "system_name": state.persona.system_name(),
                "mythic_codename": mythic,
            });

            let subflow_id = call_id.clone();
            {
                let mut states = state.subflow_states.write().await;
                states.insert(
                    subflow_id.clone(),
                    SubflowState {
                        call_id: subflow_id.clone(),
                        agent_id: agent_id.to_string(),
                        task_id: None,
                        state: "queued".to_string(),
                        message: None,
                    },
                );
            }

            let (result_tx, result_rx) = oneshot::channel();
            let state_clone = state.clone();
            let prompt_owned = prompt.to_string();
            let agent_id_owned = agent_id.to_string();
            let subflow_id_for_task = subflow_id.clone();
            let handle = tokio::spawn(async move {
                {
                    let mut states = state_clone.subflow_states.write().await;
                    if let Some(s) = states.get_mut(&subflow_id_for_task) {
                        s.state = "running".to_string();
                    }
                }

                let task_output = {
                    let mut agent = agent_arc.write().await;
                    agent
                        .query_with_task(prompt_owned, config, Some(task_meta))
                        .await
                };

                {
                    let mut states = state_clone.subflow_states.write().await;
                    if let Some(s) = states.get_mut(&subflow_id_for_task) {
                        s.task_id = Some(task_output.task_id.clone());
                        s.state = "completed".to_string();
                        s.message = None;
                    }
                }

                let _ = result_tx.send(task_output);
                tracing::debug!("查询子流程完成: call_id={}, agent_id={}", subflow_id_for_task, agent_id_owned);
            });

            {
                let mut handles = state.subflow_handles.write().await;
                handles.insert(subflow_id.clone(), handle);
            }

            let task_output = match result_rx.await {
                Ok(v) => v,
                Err(_) => {
                    let mut states = state.subflow_states.write().await;
                    if let Some(s) = states.get_mut(&subflow_id) {
                        s.state = "failed".to_string();
                        s.message = Some("子流程异常终止".to_string());
                    }
                    return serde_json::to_string(&MethodResponse::error(
                        &call_id,
                        "查询子流程异常终止".to_string(),
                    ))
                    .unwrap_or_else(|_| json!({"error": "序列化响应失败"}).to_string());
                }
            };

            {
                let mut handles = state.subflow_handles.write().await;
                if let Some(h) = handles.remove(&subflow_id) {
                    let _ = h.await;
                }
            }

            state.metrics.inc_requests();
            MethodResponse::success(&call_id, json!({
                "result": task_output.output,
                "task_id": task_output.task_id,
                "agent_id": agent_id,
            }))
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

        // 待处理工具批准列表（Web 轮询 / 第二连接）
        "approvals_pending" => {
            match &state.approval_broker {
                Some(b) => {
                    let pending = b.list_pending().await;
                    MethodResponse::success(&call_id, json!({ "pending": pending }))
                }
                None => MethodResponse::error(&call_id, "未启用人工批准 Broker"),
            }
        }

        // 响应工具批准：params: { "approval_id": "...", "approved": true|false }
        "approval_respond" => {
            let approval_id = params
                .get("approval_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let approved = params
                .get("approved")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if approval_id.is_empty() {
                MethodResponse::error(&call_id, "缺少 approval_id")
            } else {
                match &state.approval_broker {
                    Some(b) => match b.respond(approval_id, approved).await {
                        Ok(()) => MethodResponse::success(&call_id, json!({ "ok": true })),
                        Err(e) => MethodResponse::error(&call_id, e),
                    },
                    None => MethodResponse::error(&call_id, "未启用人工批准 Broker"),
                }
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

/// 按 `trace_id` 读取审计日志中的步骤（JSON 数组，顺序即执行顺序）
async fn get_audit_trace_handler(
    Path(trace_id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let obs = &state.settings.observability;
    if !obs.audit_log_enabled {
        return Err((
            StatusCode::NOT_FOUND,
            "audit_log_enabled 为 false".to_string(),
        ));
    }

    let path = std::path::PathBuf::from(&obs.audit_log_path);
    let tid = trace_id.clone();
    let steps = tokio::task::spawn_blocking(move || read_audit_steps_for_trace(&path, &tid))
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    Ok(Json(json!({
        "trace_id": trace_id,
        "steps": steps,
    })))
}

/// 列出待人工批准的工具调用
async fn list_pending_approvals_handler(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let Some(b) = state.approval_broker.as_ref() else {
        return Err(StatusCode::NOT_FOUND);
    };
    let pending = b.list_pending().await;
    Ok(Json(json!({ "pending": pending })))
}

#[derive(Deserialize)]
struct ApprovalRespondBody {
    approved: bool,
}

/// 提交批准或拒绝
async fn respond_approval_handler(
    Path(approval_id): Path<String>,
    State(state): State<AppState>,
    Json(body): Json<ApprovalRespondBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let Some(b) = state.approval_broker.as_ref() else {
        return Err((StatusCode::NOT_FOUND, "未配置批准 Broker".into()));
    };
    b.respond(&approval_id, body.approved)
        .await
        .map_err(|e| (StatusCode::NOT_FOUND, e))?;
    Ok(Json(json!({ "ok": true, "approval_id": approval_id })))
}

fn read_audit_steps_for_trace(
    path: &std::path::Path,
    trace_id: &str,
) -> Result<Vec<serde_json::Value>, String> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let data = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    const MAX: usize = 32 * 1024 * 1024;
    if data.len() > MAX {
        return Err(format!("审计文件超过 {} 字节上限", MAX));
    }
    let mut out = Vec::new();
    for line in data.lines() {
        if line.is_empty() {
            continue;
        }
        let v: serde_json::Value = serde_json::from_str(line).map_err(|e| e.to_string())?;
        if v.get("trace_id").and_then(|x| x.as_str()) == Some(trace_id) {
            out.push(v);
        }
    }
    Ok(out)
}
