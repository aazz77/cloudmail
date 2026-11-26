use axum::extract::{Query, State};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

use crate::handlers::{ok_value, AuthUser};
use crate::models::AppState;

#[derive(Deserialize)]
pub struct AddRequest {
    pub code: String,
    pub roleId: i64,
    pub count: i64,
    pub expireTime: String,
}

/// 原版 regKeyService.add：code/count/expireTime 必填 + 角色存在 + 时间转 UTC。
fn parse_utc(raw: &str) -> Option<String> {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(raw) {
        return Some(dt.with_timezone(&chrono::Utc).format("%Y-%m-%d %H:%M:%S").to_string());
    }
    for fmt in ["%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M:%S", "%Y-%m-%d"] {
        if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(raw, fmt)
            .or_else(|_| chrono::NaiveDate::parse_from_str(raw, fmt).map(|d| d.and_hms_opt(0, 0, 0).unwrap()))
        {
            return Some(naive.format("%Y-%m-%d %H:%M:%S").to_string());
        }
    }
    None
}

pub async fn add(State(state): State<Arc<AppState>>, user: AuthUser, Json(req): Json<AddRequest>) -> Response {
    if req.code.is_empty() || req.count <= 0 {
        return err(400, "emptyRegKey");
    }
    if req.expireTime.is_empty() {
        return err(400, "emptyRegKeyExpire");
    }
    let expire_utc = match parse_utc(&req.expireTime) {
        Some(v) => v,
        None => return err(400, "emptyRegKeyExpire"),
    };
    let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM reg_key WHERE code=? COLLATE NOCASE")
        .bind(&req.code).fetch_one(&state.pool).await.unwrap_or(0);
    if exists > 0 {
        return err(400, "isExistRegKye");
    }
    let role: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM role WHERE role_id=?")
        .bind(req.roleId).fetch_one(&state.pool).await.unwrap_or(0);
    if role == 0 {
        return err(400, "roleNotExist");
    }
    let result = sqlx::query("INSERT INTO reg_key(code,count,role_id,user_id,expire_time) VALUES(?,?,?,?,?)")
        .bind(&req.code).bind(req.count).bind(req.roleId).bind(user.user_id).bind(&expire_utc)
        .execute(&state.pool).await;
    if let Err(e) = result {
        tracing::error!("add reg key failed: {e}");
        return err(500, "addRegKeyFailed");
    }
    ok_value(json!(null))
}

/// 原版 regKeyService.list：code 前缀过滤 + roleName + 过期置 null。
pub async fn list(State(state): State<Arc<AppState>>, _user: AuthUser, Query(q): Query<serde_json::Value>) -> Response {
    let code = q.get("code").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let rows = sqlx::query(
        "SELECT r.rege_key_id,r.code,r.count,r.role_id,r.user_id,r.expire_time,r.create_time,ro.name \
         FROM reg_key AS r LEFT JOIN role AS ro ON ro.role_id=r.role_id \
         ORDER BY r.rege_key_id DESC",
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();
    let mut list: Vec<Value> = Vec::new();
    for row in &rows {
        use sqlx::Row;
        let row_code: String = row.get("code");
        if !code.is_empty() && !row_code.starts_with(&code) {
            continue;
        }
        let raw_expire: Option<String> = row.get("expire_time");
        let mut expired = false;
        if let Some(t) = raw_expire.as_deref() {
            let past: i64 = sqlx::query_scalar("SELECT datetime(?) <= datetime('now')")
                .bind(t).fetch_one(&state.pool).await.unwrap_or(0);
            expired = past == 1;
        }
        list.push(json!({
            "regKeyId": row.get::<i64, _>("rege_key_id"),
            "code": row_code,
            "count": row.get::<i64, _>("count"),
            "roleId": row.get::<i64, _>("role_id"),
            "userId": row.get::<i64, _>("user_id"),
            "expireTime": if expired { Value::Null } else { raw_expire.map(Value::String).unwrap_or(Value::Null) },
            "createTime": row.get::<Option<String>, _>("create_time"),
            "roleName": row.get::<Option<String>, _>("name").unwrap_or_default(),
        }));
    }
    ok_value(Value::Array(list))
}

/// 原版 regKeyService.history → userService.listByRegKeyId：
/// 直接返回 `{email, createTime}` 数组，不是 {list,total}。
pub async fn history(State(state): State<Arc<AppState>>, _user: AuthUser, Query(q): Query<serde_json::Value>) -> Response {
    // Query 参数是字符串，需兼容 `"regKeyId=2"` 与 JSON 数字两种形态。
    let id = q
        .get("regKeyId")
        .and_then(|v| v.as_i64().or_else(|| v.as_str().and_then(|s| s.parse::<i64>().ok())))
        .unwrap_or(0);
    let rows: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT email,create_time FROM user WHERE reg_key_id=? ORDER BY user_id DESC",
    )
    .bind(id).fetch_all(&state.pool).await.unwrap_or_default();
    ok_value(Value::Array(rows.into_iter().map(|r| json!({
        "email": r.0, "createTime": r.1,
    })).collect()))
}

#[derive(Deserialize, Default)]
pub struct IdsQuery { #[serde(default)] pub regKeyIds: Option<String> }

pub async fn delete(State(state): State<Arc<AppState>>, _user: AuthUser, Query(q): Query<IdsQuery>) -> Response {
    let ids = q.regKeyIds.unwrap_or_default().split(',').filter_map(|v| v.trim().parse::<i64>().ok()).collect::<Vec<_>>();
    if !ids.is_empty() {
        let ph = vec!["?"; ids.len()].join(",");
        let sql = format!("DELETE FROM reg_key WHERE rege_key_id IN ({ph})");
        let query = ids.iter().fold(sqlx::query(&sql), |q,id| q.bind(*id));
        let _ = query.execute(&state.pool).await;
    }
    ok_value(json!(null))
}

pub async fn clear_unused(State(state): State<Arc<AppState>>, _user: AuthUser) -> Response {
    let _ = sqlx::query("DELETE FROM reg_key WHERE count<=0 OR datetime(expire_time)<=datetime('now')")
        .execute(&state.pool).await;
    ok_value(json!(null))
}

fn err(code: u16, message: &str) -> Response {
    (axum::http::StatusCode::OK, Json(json!({"code":code,"message":message}))).into_response()
}
