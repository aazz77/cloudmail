use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::Response;
use axum::Json;
use base64::Engine;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::sync::Arc;

use crate::handlers::{ok_value, AuthUser};
use crate::models::AppState;

use super::common::snake_to_camel;
use super::verify;

/// 设置列名（snake_case，与建表迁移一致）。全部列都会按 camelCase 返回。
const SETTING_COLUMNS: &[&str] = &[
    "id", "register", "receive", "title", "many_email", "add_email", "auto_refresh",
    "add_email_verify", "register_verify", "reg_verify_count", "add_verify_count",
    "send", "r2_domain", "secret_key", "site_key", "reg_key", "background",
    "tg_bot_token", "tg_chat_id", "tg_bot_status", "forward_email", "forward_status",
    "rule_email", "rule_type", "login_opacity", "resend_tokens", "notice_title",
    "notice_content", "notice_type", "notice_duration", "notice_position",
    "notice_offset", "notice_width", "notice", "no_recipient", "login_domain",
    "bucket", "region", "endpoint", "s3_access_key", "s3_secret_key",
    "force_path_style", "custom_domain", "tg_msg_from", "tg_msg_to", "tg_msg_text",
    "min_email_prefix", "email_prefix_filter", "black_subject", "black_content",
    "black_from", "ai_code", "sync_delete", "ai_code_filter", "linuxdo_client_id",
    "linuxdo_client_secret", "linuxdo_switch", "github_client_id",
    "github_client_secret", "github_switch", "google_client_id", "google_client_secret",
    "google_switch", "auto_clean_days", "auto_clean_exclude", "webhook_url",
    "webhook_status", "webhook_retry", "webhook_secret", "login_verify",
    "login_verify_count",
];

/// 读取原始设置行并转换为 camelCase JSON（对齐 Drizzle setting 实体）。
async fn setting_camel(state: &AppState) -> Value {
    let row = sqlx::query("SELECT * FROM setting WHERE id=1")
        .fetch_optional(&state.pool)
        .await
        .ok()
        .flatten();
    let row = match row {
        Some(row) => row,
        None => return json!({}),
    };
    use sqlx::Row;
    let mut map = Map::new();
    for col in SETTING_COLUMNS {
        let value = row
            .try_get::<Option<String>, _>(col)
            .ok()
            .flatten()
            .map(Value::String)
            .or_else(|| row.try_get::<Option<i64>, _>(col).ok().flatten().map(|v| json!(v)))
            .or_else(|| {
                row.try_get::<Option<f64>, _>(col)
                    .ok()
                    .flatten()
                    .map(|v| json!(v))
            })
            .unwrap_or(Value::Null);
        map.insert(snake_to_camel(col), value);
    }
    Value::Object(map)
}

/// 基础加工（原版 settingService.query）：
/// domainList / projectLink / emailPrefixFilter 数组化 / autoRefresh 归一化 /
/// loginVerify 缺省值 / resendTokens 解析为对象。
async fn processed_setting(state: &AppState) -> Value {
    let mut setting = setting_camel(state).await;
    let obj = setting.as_object_mut().unwrap();

    // 兜底缺省值
    if !obj.contains_key("loginVerify") || obj["loginVerify"].is_null() {
        obj.insert("loginVerify".into(), json!(1));
    }
    if !obj.contains_key("loginVerifyCount") || obj["loginVerifyCount"].is_null() {
        obj.insert("loginVerifyCount".into(), json!(1));
    }

    // autoRefresh: 1 关闭，2 开启
    let auto_refresh = obj.get("autoRefresh").and_then(|v| v.as_i64()).unwrap_or(0);
    obj.insert("autoRefresh".into(), json!(if auto_refresh == 1 { 1 } else { 2 }));

    // emailPrefixFilter 拆数组（去空项）
    let filter = obj
        .get("emailPrefixFilter")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    obj.insert(
        "emailPrefixFilter".into(),
        Value::Array(
            filter
                .split(',')
                .map(|s| json!(s.trim()))
                .filter(|s| s.as_str().map(|v| !v.is_empty()).unwrap_or(false))
                .collect(),
        ),
    );

    // resendTokens 解析为对象
    let tokens_raw = obj
        .get("resendTokens")
        .and_then(|v| v.as_str())
        .unwrap_or("{}")
        .to_string();
    let tokens: Value = serde_json::from_str(&tokens_raw).unwrap_or(json!({}));
    obj.insert("resendTokens".into(), if tokens.is_object() { tokens } else { json!({}) });

    // domainList: 配置域名加 @ 前缀（原版来自 env.domain）
    obj.insert(
        "domainList".into(),
        Value::Array(
            state
                .config
                .domains
                .iter()
                .map(|d| json!(format!("@{d}")))
                .collect(),
        ),
    );
    obj.insert("projectLink".into(), json!(true));
    setting
}

/// 密钥脱敏：保留前 N 位后追加 `******`（原版 settingService.get）。
fn mask(value: Option<&str>, keep: usize) -> Value {
    match value {
        Some(v) if !v.is_empty() => {
            let head: String = v.chars().take(keep).collect();
            json!(format!("{head}******"))
        }
        _ => json!(null),
    }
}

/// 完整加工（原版 settingService.get）：密钥脱敏 + 按 IP 的人机验证开关状态 +
/// 存储类型标记。`/setting/query` 与 `websiteConfig` 共用。
pub async fn get_processed(state: &Arc<AppState>, ip: &str) -> Value {
    let mut setting = processed_setting(state).await;
    let obj = setting.as_object_mut().unwrap();

    // 密钥脱敏（resendTokens 每个域名的 token 保留前 12 位）
    let secret_key = obj.get("secretKey").and_then(|v| v.as_str()).map(|s| s.to_string());
    obj.insert("secretKey".into(), mask(secret_key.as_deref(), 6));
    let s3_access = obj.get("s3AccessKey").and_then(|v| v.as_str()).map(|s| s.to_string());
    obj.insert("s3AccessKey".into(), mask(s3_access.as_deref(), 12));
    let s3_secret = obj.get("s3SecretKey").and_then(|v| v.as_str()).map(|s| s.to_string());
    obj.insert("s3SecretKey".into(), mask(s3_secret.as_deref(), 12));
    let tg_token = obj.get("tgBotToken").and_then(|v| v.as_str()).map(|s| s.to_string());
    obj.insert("tgBotToken".into(), mask(tg_token.as_deref(), 20));

    if let Some(tokens) = obj.get_mut("resendTokens").and_then(|v| v.as_object_mut()) {
        for (_, value) in tokens.iter_mut() {
            if let Some(token) = value.as_str().map(|s| s.to_string()) {
                *value = mask(Some(&token), 12);
            }
        }
    }

    // 人机验证开关状态：按请求方 IP 的 verify_record 计数判断
    let reg_verify_count = obj.get("regVerifyCount").and_then(|v| v.as_i64()).unwrap_or(1);
    let add_verify_count = obj.get("addVerifyCount").and_then(|v| v.as_i64()).unwrap_or(1);
    let login_verify_count = obj.get("loginVerifyCount").and_then(|v| v.as_i64()).unwrap_or(1);
    obj.insert(
        "regVerifyOpen".into(),
        json!(verify::is_open_reg_verify(state, ip, reg_verify_count).await),
    );
    obj.insert(
        "addVerifyOpen".into(),
        json!(verify::is_open_add_verify(state, ip, add_verify_count).await),
    );
    obj.insert(
        "loginVerifyOpen".into(),
        json!(verify::is_open_login_verify(state, ip, login_verify_count).await),
    );

    // 存储类型：S3 已配置则 S3，否则本地文件存储替代 R2
    let is_s3 = ["bucket", "endpoint", "s3AccessKey", "s3SecretKey"].iter().all(|k| {
        obj.get(*k)
            .and_then(|v| v.as_str())
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false)
    });
    obj.insert("storageType".into(), json!(if is_s3 { "S3" } else { "R2" }));
    // 本地文件存储等价于原版 R2 绑定始终存在；无 CF Email 转发通道
    obj.insert("hasR2".into(), json!(true));
    obj.insert("hasCfEmail".into(), json!(false));
    setting
}

pub async fn query(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    let ip = crate::utils::client_ip(&headers, "Unknown");
    ok_value(get_processed(&state, &ip).await)
}

pub async fn website_config(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    let ip = crate::utils::client_ip(&headers, "Unknown");
    let setting = get_processed(&state, &ip).await;
    let login_domain = setting["loginDomain"].as_i64().unwrap_or(0);
    let domain_list = if login_domain == 1 { Value::Array(vec![]) } else { setting["domainList"].clone() };
    ok_value(json!({
        "register": setting["register"], "title": setting["title"],
        "manyEmail": setting["manyEmail"], "addEmail": setting["addEmail"],
        "autoRefresh": setting["autoRefresh"], "addEmailVerify": setting["addEmailVerify"],
        "registerVerify": setting["registerVerify"], "loginVerify": setting["loginVerify"],
        "send": setting["send"], "r2Domain": setting["r2Domain"], "siteKey": setting["siteKey"],
        "background": setting["background"], "loginOpacity": setting["loginOpacity"],
        "domainList": domain_list, "regKey": setting["regKey"],
        "regVerifyOpen": setting["regVerifyOpen"], "addVerifyOpen": setting["addVerifyOpen"],
        "loginVerifyOpen": setting["loginVerifyOpen"],
        "noticeTitle": setting["noticeTitle"], "noticeContent": setting["noticeContent"],
        "noticeType": setting["noticeType"], "noticeDuration": setting["noticeDuration"],
        "noticePosition": setting["noticePosition"], "noticeWidth": setting["noticeWidth"],
        "noticeOffset": setting["noticeOffset"], "notice": setting["notice"],
        "loginDomain": setting["loginDomain"], "minEmailPrefix": setting["minEmailPrefix"],
        "projectLink": setting["projectLink"],
        "linuxdoClientId": setting["linuxdoClientId"], "linuxdoSwitch": setting["linuxdoSwitch"],
        "githubClientId": setting["githubClientId"], "githubSwitch": setting["githubSwitch"],
        "googleClientId": setting["googleClientId"], "googleSwitch": setting["googleSwitch"]
    }))
}

pub async fn set(State(state): State<Arc<AppState>>, _user: AuthUser, Json(req): Json<Value>) -> Response {
    let mut req = req.as_object().cloned().unwrap_or_default();

    // resendTokens 增量合并并删除空值（原版 settingService.set）
    if let Some(tokens) = req.remove("resendTokens") {
        let current: Map<String, Value> = sqlx::query("SELECT resend_tokens FROM setting WHERE id=1")
            .fetch_optional(&state.pool)
            .await
            .ok()
            .flatten()
            .and_then(|row| {
                use sqlx::Row;
                let raw: Option<String> = row.try_get("resend_tokens").ok().flatten();
                raw.and_then(|s| serde_json::from_str::<Value>(&s).ok())
            })
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default();
        let mut merged = current;
        if let Some(incoming) = tokens.as_object() {
            for (k, v) in incoming {
                let empty = v.as_str().map(|s| s.is_empty()).unwrap_or(v.is_null());
                if empty {
                    merged.remove(k);
                } else {
                    merged.insert(k.clone(), v.clone());
                }
            }
        }
        req.insert(
            "resendTokens".into(),
            Value::String(Value::Object(merged).to_string()),
        );
    }

    // 数组字段转字符串
    for key in ["emailPrefixFilter", "aiCodeFilter"] {
        if let Some(v) = req.remove(key) {
            if let Some(list) = v.as_array() {
                let joined = list
                    .iter()
                    .filter_map(|x| x.as_str())
                    .collect::<Vec<_>>()
                    .join(",");
                req.insert(key.into(), json!(joined));
            } else {
                req.insert(key.into(), v);
            }
        }
    }

    // autoRefresh 归一化：1 关闭，其余一律 2
    if let Some(v) = req.remove("autoRefresh") {
        let normalized = if v.as_i64().map(|n| n == 1).unwrap_or(false) { 1 } else { 2 };
        req.insert("autoRefresh".into(), json!(normalized));
    }

    // loginVerifyCount 下限 1
    if let Some(v) = req.get_mut("loginVerifyCount") {
        let n = v.as_i64().unwrap_or(1);
        if n < 1 {
            *v = json!(1);
        }
    }

    let mut sets = Vec::new();
    let mut values: Vec<Value> = Vec::new();
    for (key, value) in req {
        let column = match key.as_str() {
            "manyEmail" => "many_email", "addEmail" => "add_email", "autoRefresh" => "auto_refresh",
            "addEmailVerify" => "add_email_verify", "registerVerify" => "register_verify",
            "regVerifyCount" => "reg_verify_count", "addVerifyCount" => "add_verify_count",
            "loginVerify" => "login_verify", "loginVerifyCount" => "login_verify_count",
            "r2Domain" => "r2_domain", "secretKey" => "secret_key", "siteKey" => "site_key",
            "regKey" => "reg_key", "background" => "background", "tgBotToken" => "tg_bot_token",
            "tgChatId" => "tg_chat_id", "tgBotStatus" => "tg_bot_status", "forwardEmail" => "forward_email",
            "forwardStatus" => "forward_status", "ruleEmail" => "rule_email", "ruleType" => "rule_type",
            "loginOpacity" => "login_opacity", "resendTokens" => "resend_tokens", "noticeTitle" => "notice_title",
            "noticeContent" => "notice_content", "noticeType" => "notice_type", "noticeDuration" => "notice_duration",
            "noticePosition" => "notice_position", "noticeOffset" => "notice_offset", "noticeWidth" => "notice_width",
            "notice" => "notice", "noRecipient" => "no_recipient", "loginDomain" => "login_domain",
            "bucket" => "bucket", "region" => "region", "endpoint" => "endpoint", "s3AccessKey" => "s3_access_key",
            "s3SecretKey" => "s3_secret_key", "forcePathStyle" => "force_path_style", "customDomain" => "custom_domain",
            "tgMsgFrom" => "tg_msg_from", "tgMsgTo" => "tg_msg_to", "tgMsgText" => "tg_msg_text",
            "minEmailPrefix" => "min_email_prefix", "emailPrefixFilter" => "email_prefix_filter",
            "blackSubject" => "black_subject", "blackContent" => "black_content", "blackFrom" => "black_from",
            "aiCode" => "ai_code", "syncDelete" => "sync_delete", "aiCodeFilter" => "ai_code_filter",
            "autoCleanDays" => "auto_clean_days", "autoCleanExclude" => "auto_clean_exclude",
            "webhookUrl" => "webhook_url", "webhookStatus" => "webhook_status", "webhookRetry" => "webhook_retry",
            "webhookSecret" => "webhook_secret", "linuxdoClientId" => "linuxdo_client_id",
            "linuxdoClientSecret" => "linuxdo_client_secret", "linuxdoSwitch" => "linuxdo_switch",
            "githubClientId" => "github_client_id", "githubClientSecret" => "github_client_secret",
            "githubSwitch" => "github_switch", "googleClientId" => "google_client_id",
            "googleClientSecret" => "google_client_secret", "googleSwitch" => "google_switch",
            _ => continue,
        };
        sets.push(format!("{column}=?"));
        values.push(value);
    }
    if !sets.is_empty() {
        let sql = format!("UPDATE setting SET {} WHERE id=1", sets.join(","));
        let mut query = sqlx::query(&sql);
        for value in values {
            query = bind_value(query, value);
        }
        let _ = query.execute(&state.pool).await;
    }
    ok_value(json!(null))
}

fn bind_value<'q>(query: sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'q>>, value: Value) -> sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'q>> {
    if let Some(v) = value.as_str() {
        query.bind(v.to_string())
    } else if let Some(v) = value.as_i64() {
        query.bind(v)
    } else if let Some(v) = value.as_f64() {
        query.bind(v)
    } else if let Some(v) = value.as_bool() {
        query.bind(if v {1i64} else {0i64})
    } else {
        query.bind(Option::<String>::None)
    }
}

pub async fn set_background(State(state): State<Arc<AppState>>, _user: AuthUser, Json(req): Json<Value>) -> Response {
    let background = req.get("background").and_then(|v| v.as_str()).unwrap_or("").to_string();
    if background.is_empty() {
        sqlx::query("UPDATE setting SET background='' WHERE id=1").execute(&state.pool).await.unwrap();
        return ok_value(json!(""));
    }
    if background.starts_with("http") {
        sqlx::query("UPDATE setting SET background=? WHERE id=1").bind(&background).execute(&state.pool).await.unwrap();
        return ok_value(json!(background));
    }
    // Data URI: `data:<mime>;base64,<payload>` — derive the extension from the
    // declared MIME type instead of assuming PNG.
    let (mime, payload) = background
        .split_once(',')
        .map(|(head, payload)| {
            let mime = head
                .strip_prefix("data:")
                .and_then(|s| s.split(';').next())
                .unwrap_or("image/png")
                .to_string();
            (mime, payload)
        })
        .unwrap_or(("image/png".to_string(), background.as_str()));
    let decoded = base64::engine::general_purpose::STANDARD.decode(payload).unwrap_or_default();
    let ext = match mime.to_lowercase().as_str() {
        "image/jpeg" | "image/jpg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/svg+xml" => "svg",
        "image/x-icon" | "image/vnd.microsoft.icon" => "ico",
        _ => "png",
    };
    let key = format!("static/background/{}.{}", hash(&decoded), ext);
    let _ = crate::storage::put_object(&state.config.storage_dir, &key, &decoded).await;
    sqlx::query("UPDATE setting SET background=? WHERE id=1").bind(&key).execute(&state.pool).await.unwrap();
    ok_value(json!(key))
}

pub async fn delete_background(State(state): State<Arc<AppState>>, _user: AuthUser) -> Response {
    sqlx::query("UPDATE setting SET background='' WHERE id=1").execute(&state.pool).await.unwrap();
    ok_value(json!(null))
}

pub async fn set_blacklist(State(state): State<Arc<AppState>>, _user: AuthUser, headers: HeaderMap, Json(req): Json<Value>) -> Response {
    for key in ["blackSubject", "blackContent", "blackFrom"] {
        if let Some(v) = req.get(key).and_then(|v| v.as_str()) {
            let column = match key { "blackSubject" => "black_subject", "blackContent" => "black_content", _ => "black_from" };
            sqlx::query(&format!("UPDATE setting SET {column}=? WHERE id=1")).bind(v).execute(&state.pool).await.unwrap();
        }
    }
    let ip = crate::utils::client_ip(&headers, "Unknown");
    ok_value(get_processed(&state, &ip).await)
}

fn hash(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new(); hasher.update(bytes);
    base64::engine::general_purpose::STANDARD.encode(hasher.finalize())
}
