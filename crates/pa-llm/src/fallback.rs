//! 主模型不可用时的保守判定逻辑。
//!
//! 仅在「模型/路由明确不可用」时建议尝试备用模型；**不会**将 429、529、
//! 泛型 5xx、网关错误等视为「主模型挂了」，避免随意切换。

/// 根据 HTTP 状态码与错误正文，判断当前失败是否属于「主模型/路由级不可用」，
/// 从而在已开启备用切换且备用模型探测通过时，才允许改用备用模型。
pub(crate) fn primary_model_unreachable_for_fallback(status: u16, message: &str) -> bool {
    let m = message.to_lowercase();
    match status {
        // 资源不存在：在 messages/chat 语义下多为未知模型或无效路由
        404 => true,
        // 服务不可用：仅当正文明确指向模型停用时启用（避免泛型503 误切）
        503 => message_indicates_model_route_unavailable(&m),
        // 错误请求：仅当同时涉及 model 且为「未知/无效」类描述时启用
        400 => message_indicates_invalid_or_unknown_model(&m),
        _ => false,
    }
}

fn message_indicates_model_route_unavailable(m: &str) -> bool {
    const NEEDLES: &[&str] = &[
        "model",
        "not available",
        "unavailable",
        "does not exist",
        "disabled",
        "deprecated",
        "no longer available",
        "模型",
        "不可用",
        "不存在",
        "已停用",
    ];
    contains_any(m, NEEDLES)
}

fn message_indicates_invalid_or_unknown_model(m: &str) -> bool {
    let has_model = m.contains("model") || m.contains("模型");
    if !has_model {
        return false;
    }
    const NEEDLES: &[&str] = &[
        "invalid",
        "unknown",
        "not found",
        "does not exist",
        "not supported",
        "incorrect",
        "invalid_model",
        "model_not_found",
        "无效",
        "不存在",
        "不支持",
    ];
    contains_any(m, NEEDLES)
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|n| haystack.contains(n))
}
