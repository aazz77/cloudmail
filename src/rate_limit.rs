//! 轻量 IP 限流中间件：按分钟固定窗口计数，超限返回 429。
//! 登录接口使用更严格阈值，防止密码爆破（与 login_attempt 双维度锁定互补）。

use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::models::AppState;

/// 每 IP 每分钟允许的请求数（全站默认）。
const DEFAULT_PER_MIN: u32 = 300;
/// 登录接口每 IP 每分钟请求数（更严）。
const LOGIN_PER_MIN: u32 = 10;

struct Window {
    minute: u64,
    count: u32,
}

pub struct IpRateLimiter {
    buckets: Mutex<HashMap<String, Window>>,
}

impl IpRateLimiter {
    pub fn new() -> Self {
        Self { buckets: Mutex::new(HashMap::new()) }
    }

    fn current_minute() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            / 60
    }

    /// 对 key 计数 +1；超过 limit 返回 false。
    fn hit(&self, key: &str, limit: u32) -> bool {
        let minute = Self::current_minute();
        let mut buckets = match self.buckets.lock() {
            Ok(b) => b,
            Err(_) => return false,
        };
        let entry = buckets.entry(key.to_string()).or_insert(Window { minute, count: 0 });
        if entry.minute != minute {
            entry.minute = minute;
            entry.count = 0;
        }
        entry.count += 1;
        entry.count <= limit
    }

    /// 定期清理过期桶，防止内存无限增长（每分钟最多一次实际清理）。
    fn sweep(&self) {
        let minute = Self::current_minute();
        if let Ok(mut buckets) = self.buckets.lock() {
            buckets.retain(|_, w| w.minute >= minute.saturating_sub(2));
        }
    }
}

/// 从请求中提取 IP（与 utils::client_ip 同策略）。
fn ip_of(headers: &HeaderMap) -> String {
    for name in ["cf-connecting-ip", "x-real-ip", "x-forwarded-for"] {
        if let Some(value) = headers.get(name).and_then(|v| v.to_str().ok()) {
            let ip = value.split(',').next().unwrap_or("").trim();
            if !ip.is_empty() {
                return ip.to_string();
            }
        }
    }
    "unknown".to_string()
}

pub async fn rate_limit(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    req: Request,
    next: Next,
) -> Response {
    let path = req.uri().path().to_string();
    let key_ip = ip_of(&headers);
    // 登录按 IP 单独限流（阈值更严）；其余按 IP 全局限流。
    let (key, limit) = if path.ends_with("/login") {
        (format!("login:{key_ip}"), LOGIN_PER_MIN)
    } else {
        (format!("global:{key_ip}"), DEFAULT_PER_MIN)
    };
    if !state.rate_limiter.hit(&key, limit) {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            axum::Json(serde_json::json!({"code": 429, "message": "rateLimited"})),
        )
            .into_response();
    }
    // 低频清理：每分钟第 0 秒附近的请求触发一次。
    if SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        % 60
        == 0
    {
        state.rate_limiter.sweep();
    }
    next.run(req).await
}
