//! Turnstile 人机校验与 IP 计数表（verify_record）。
//! 对齐原版 `service/turnstile-service.js` 与 `service/verify-record-service.js`。
//! Turnstile 本质是一次 siteverify HTTP POST，普通服务器用 reqwest 即可实现，
//! 不属于 Cloudflare 专属能力。

use sqlx::SqlitePool;
use std::sync::Arc;

use crate::models::AppState;

/// verify_record.type 常量（原版 verifyRecordType）。
pub const VR_REG: i64 = 0;
pub const VR_ADD: i64 = 1;
pub const VR_LOGIN: i64 = 2;

const SITEVERIFY_URL: &str = "https://challenges.cloudflare.com/turnstile/v0/siteverify";

/// 校验 Turnstile token。secretKey 取自设置表（原版 turnstileService.verify）。
/// 失败返回 i18n 风格错误键：`emptyBotToken` / `botVerifyFail`。
pub async fn verify_turnstile(state: &Arc<AppState>, token: &str, ip: &str) -> Result<(), String> {
    if token.is_empty() {
        return Err("emptyBotToken".into());
    }
    let setting = super::auth::current_setting(state).await;
    let secret = setting
        .get("secret_key")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let form = [
        ("secret", secret.as_str()),
        ("response", token),
        ("remoteip", ip),
    ];
    let client = reqwest::Client::new();
    let result: Result<serde_json::Value, String> = async {
        let resp = client
            .post(SITEVERIFY_URL)
            .form(&form)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        resp.json::<serde_json::Value>().await.map_err(|e| e.to_string())
    }
    .await;

    match result {
        Ok(body) => {
            if body.get("success").and_then(|v| v.as_bool()).unwrap_or(false) {
                Ok(())
            } else {
                Err("botVerifyFail".into())
            }
        }
        Err(e) => {
            tracing::warn!("turnstile siteverify failed: {e}");
            Err("botVerifyFail".into())
        }
    }
}

/// 查询某 IP 某类型的计数值；不存在返回 0。
async fn record_count(pool: &SqlitePool, ip: &str, vr_type: i64) -> i64 {
    sqlx::query_scalar("SELECT COALESCE(MAX(count),0) FROM verify_record WHERE ip=? AND type=?")
        .bind(ip)
        .bind(vr_type)
        .fetch_one(pool)
        .await
        .unwrap_or(0)
}

/// 登录尝试原子扣减（原版 consumeLoginAttempt）：小于 limit 时 count+1 并返回 true。
/// 返回 false 表示本次尝试已耗尽免费额度，需要人机验证。
pub async fn consume_login_attempt(state: &Arc<AppState>, ip: &str, limit: i64) -> bool {
    let _ = sqlx::query(
        "INSERT INTO verify_record(ip,type,count) \
         SELECT ?,?,0 WHERE NOT EXISTS (SELECT 1 FROM verify_record WHERE ip=? AND type=?)",
    )
    .bind(ip)
    .bind(VR_LOGIN)
    .bind(ip)
    .bind(VR_LOGIN)
    .execute(&state.pool)
    .await;
    let updated = sqlx::query(
        "UPDATE verify_record SET count=count+1, update_time=datetime('now') \
         WHERE ip=? AND type=? AND count<?",
    )
    .bind(ip)
    .bind(VR_LOGIN)
    .bind(limit)
    .execute(&state.pool)
    .await;
    matches!(updated, Ok(r) if r.rows_affected() > 0)
}

/// 注册验证是否已触发（原版 isOpenRegVerify）。
pub async fn is_open_reg_verify(state: &Arc<AppState>, ip: &str, reg_verify_count: i64) -> bool {
    record_count(&state.pool, ip, VR_REG).await >= reg_verify_count
}

/// 加邮箱验证是否已触发（原版 isOpenAddVerify）。
pub async fn is_open_add_verify(state: &Arc<AppState>, ip: &str, add_verify_count: i64) -> bool {
    record_count(&state.pool, ip, VR_ADD).await >= add_verify_count
}

/// 登录验证是否已触发。
pub async fn is_open_login_verify(state: &Arc<AppState>, ip: &str, login_verify_count: i64) -> bool {
    record_count(&state.pool, ip, VR_LOGIN).await >= login_verify_count
}

/// 注册计数 +1，返回最新计数（原版 increaseRegCount）。
pub async fn increase_reg_count(state: &Arc<AppState>, ip: &str) -> i64 {
    increase(state, ip, VR_REG).await
}

/// 加邮箱计数 +1，返回最新计数（原版 increaseAddCount）。
pub async fn increase_add_count(state: &Arc<AppState>, ip: &str) -> i64 {
    increase(state, ip, VR_ADD).await
}

async fn increase(state: &Arc<AppState>, ip: &str, vr_type: i64) -> i64 {
    let updated = sqlx::query(
        "UPDATE verify_record SET count=count+1, update_time=datetime('now') WHERE ip=? AND type=?",
    )
    .bind(ip)
    .bind(vr_type)
    .execute(&state.pool)
    .await;
    match updated {
        Ok(r) if r.rows_affected() > 0 => {}
        _ => {
            let _ = sqlx::query(
                "INSERT INTO verify_record(ip,type,count) VALUES(?,?,1) \
                 ON CONFLICT DO NOTHING",
            )
            .bind(ip)
            .bind(vr_type)
            .execute(&state.pool)
            .await;
        }
    }
    record_count(&state.pool, ip, vr_type).await
}
