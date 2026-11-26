use axum::extract::{Query, State};
use axum::response::Response;
use axum::Json;
use serde_json::{json, Value};
use std::sync::Arc;

use crate::handlers::{err, ok_value, AuthUser};
use crate::models::AppState;

/// 原版 verifyUtils：邮箱或域名格式（`*` 仅 banEmail 允许）。
fn valid_ban_item(value: &str) -> bool {
    if value == "*" {
        return true;
    }
    if value.contains('@') {
        return crate::utils::valid_email(value);
    }
    let parts: Vec<&str> = value.split('.').collect();
    parts.len() >= 2
        && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
        && parts.last().map(|p| p.chars().all(|c| c.is_ascii_alphabetic())).unwrap_or(false)
}

fn validate_name_ban(name: &str, ban: &[Value]) -> Option<Response> {
    if name.is_empty() {
        return Some(err(400, "emptyRoleName"));
    }
    if ban.iter().filter_map(|v| v.as_str()).any(|v| !valid_ban_item(v)) {
        return Some(err(400, "notEmail"));
    }
    None
}

pub async fn add(State(state): State<Arc<AppState>>, user: AuthUser, Json(req): Json<Value>) -> Response {
    let name = req.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let perms = req.get("permIds").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let ban = req.get("banEmail").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let domains = req.get("availDomain").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    if let Some(resp) = validate_name_ban(&name, &ban) {
        return resp;
    }
    let result = sqlx::query(
        "INSERT INTO role(name,key,description,ban_email,ban_email_type,avail_domain,sort,user_id,send_count,send_type,account_count) \
         VALUES(?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind(&name)
    .bind(req.get("key").and_then(|v| v.as_str()).unwrap_or(""))
    .bind(req.get("description").and_then(|v| v.as_str()))
    .bind(join(&ban))
    .bind(req.get("banEmailType").and_then(|v| v.as_i64()).unwrap_or(0))
    .bind(join(&domains))
    .bind(req.get("sort").and_then(|v| v.as_f64()))
    .bind(user.user_id)
    .bind(req.get("sendCount").and_then(|v| v.as_i64()))
    .bind(req.get("sendType").and_then(|v| v.as_str()).unwrap_or("count"))
    .bind(req.get("accountCount").and_then(|v| v.as_i64()))
    .execute(&state.pool)
    .await;
    let role_id = match result {
        Ok(r) => r.last_insert_rowid(),
        Err(e) => {
            tracing::error!("add role failed: {e}");
            return err(500, "addRoleFailed");
        }
    };
    for perm in perms {
        if let Some(id) = perm.as_i64() {
            let _ = sqlx::query("INSERT INTO role_perm(role_id,perm_id) VALUES(?,?)")
                .bind(role_id).bind(id).execute(&state.pool).await;
        }
    }
    ok_value(json!(null))
}

pub async fn set(State(state): State<Arc<AppState>>, _user: AuthUser, Json(req): Json<Value>) -> Response {
    let id = req.get("roleId").and_then(|v| v.as_i64().or_else(|| v.as_str().and_then(|s| s.parse::<i64>().ok()))).unwrap_or(0);
    let name = req.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let ban = req.get("banEmail").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let domains = req.get("availDomain").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    if let Some(resp) = validate_name_ban(name, &ban) {
        return resp;
    }
    // 原版 setRole 把表单参数整体写回（含 sendCount/sendType/accountCount），
    // 只剔除 isDefault；此前 Rust 漏掉这三项导致额度配置无法保存。
    let result = sqlx::query(
        "UPDATE role SET name=?,key=?,description=?,ban_email=?,ban_email_type=?,avail_domain=?,sort=?, \
         send_count=?,send_type=?,account_count=? WHERE role_id=?",
    )
    .bind(name)
    .bind(req.get("key").and_then(|v| v.as_str()).unwrap_or(""))
    .bind(req.get("description").and_then(|v| v.as_str()))
    .bind(join(&ban))
    .bind(req.get("banEmailType").and_then(|v| v.as_i64()).unwrap_or(0))
    .bind(join(&domains))
    .bind(req.get("sort").and_then(|v| v.as_f64()))
    .bind(req.get("sendCount").and_then(|v| v.as_i64()))
    .bind(req.get("sendType").and_then(|v| v.as_str()).unwrap_or("count"))
    .bind(req.get("accountCount").and_then(|v| v.as_i64()))
    .bind(id)
    .execute(&state.pool)
    .await;
    if let Err(e) = result {
        tracing::error!("set role failed: {e}");
        return err(500, "setRoleFailed");
    }
    let _ = sqlx::query("DELETE FROM role_perm WHERE role_id=?").bind(id).execute(&state.pool).await;
    if let Some(perms) = req.get("permIds").and_then(|v| v.as_array()) {
        for p in perms {
            if let Some(pid) = p.as_i64() {
                let _ = sqlx::query("INSERT INTO role_perm(role_id,perm_id) VALUES(?,?)")
                    .bind(id).bind(pid).execute(&state.pool).await;
            }
        }
    }
    ok_value(json!(null))
}

pub async fn set_default(State(state): State<Arc<AppState>>, _user: AuthUser, Json(req): Json<Value>) -> Response {
    let id = req.get("roleId").and_then(|v| v.as_i64().or_else(|| v.as_str().and_then(|s| s.parse::<i64>().ok()))).unwrap_or(0);
    let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM role WHERE role_id=?")
        .bind(id).fetch_one(&state.pool).await.unwrap_or(0);
    if exists == 0 {
        return err(400, "roleNotExist");
    }
    let _ = sqlx::query("UPDATE role SET is_default=0").execute(&state.pool).await;
    let _ = sqlx::query("UPDATE role SET is_default=1 WHERE role_id=?").bind(id).execute(&state.pool).await;
    ok_value(json!(null))
}

/// 原版 roleList：完整实体字段 + 数组化 banEmail/availDomain + permIds（仅按钮权限）。
pub async fn list(State(state): State<Arc<AppState>>, _user: AuthUser) -> Response {
    let rows = sqlx::query(
        "SELECT role_id,name,key,description,ban_email,ban_email_type,avail_domain,sort,is_default, \
         create_time,user_id,send_count,send_type,account_count FROM role ORDER BY sort,role_id",
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();
    let perms: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT rp.role_id,rp.perm_id FROM role_perm AS rp \
         LEFT JOIN perm AS p ON p.perm_id=rp.perm_id WHERE p.type=2",
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();
    let list: Vec<Value> = rows.iter().map(|r| {
        use sqlx::Row;
        let role_id: i64 = r.get("role_id");
        json!({
            "roleId": role_id,
            "name": r.get::<String, _>("name"),
            "key": r.get::<Option<String>, _>("key"),
            "description": r.get::<Option<String>, _>("description"),
            "banEmail": split(&r.get::<String, _>("ban_email")),
            "banEmailType": r.get::<i64, _>("ban_email_type"),
            "availDomain": split(&r.get::<String, _>("avail_domain")),
            "sort": r.try_get::<Option<f64>, _>("sort").ok().flatten()
                .or_else(|| r.try_get::<Option<i64>, _>("sort").ok().flatten().map(|v| v as f64)),
            "isDefault": r.get::<i64, _>("is_default"),
            "createTime": r.get::<Option<String>, _>("create_time"),
            "userId": r.get::<Option<i64>, _>("user_id"),
            "sendCount": r.get::<Option<i64>, _>("send_count"),
            "sendType": r.get::<Option<String>, _>("send_type"),
            "accountCount": r.get::<Option<i64>, _>("account_count"),
            "permIds": perms.iter().filter(|(rid, _)| *rid == role_id).map(|(_, pid)| *pid).collect::<Vec<_>>(),
        })
    }).collect();
    ok_value(Value::Array(list))
}

/// 原版 permService.tree：pid=0 为父节点，子节点挂到 children。
pub async fn tree(State(state): State<Arc<AppState>>, _user: AuthUser) -> Response {
    let rows: Vec<(i64, String, Option<String>, i64, Option<f64>)> = sqlx::query_as(
        "SELECT perm_id,name,perm_key,pid,sort FROM perm ORDER BY sort,perm_id",
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();
    let mut parents: Vec<Value> = Vec::new();
    let mut children: Vec<Value> = Vec::new();
    for (id, name, key, pid, sort) in rows {
        let node = json!({
            "permId": id, "name": name, "permKey": key, "pid": pid, "sort": sort,
        });
        if pid == 0 {
            parents.push(node);
        } else {
            children.push(node);
        }
    }
    for parent in parents.iter_mut() {
        let pid = parent.get("permId").and_then(|v| v.as_i64()).unwrap_or(0);
        let kids: Vec<Value> = children.iter()
            .filter(|c| c.get("pid").and_then(|v| v.as_i64()).unwrap_or(0) == pid)
            .cloned()
            .collect();
        if let Some(obj) = parent.as_object_mut() {
            obj.insert("children".into(), Value::Array(kids));
        }
    }
    ok_value(Value::Array(parents))
}

pub async fn select_use(State(state): State<Arc<AppState>>, _user: AuthUser) -> Response {
    let rows: Vec<(i64, String, i64)> = sqlx::query_as(
        "SELECT role_id,name,is_default FROM role ORDER BY sort,role_id",
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();
    ok_value(Value::Array(rows.into_iter().map(|r| json!({
        "roleId": r.0, "name": r.1, "isDefault": r.2,
    })).collect()))
}

pub async fn delete(State(state): State<Arc<AppState>>, _user: AuthUser, Query(q): Query<serde_json::Value>) -> Response {
    let id = q.get("roleId").and_then(|v| v.as_i64().or_else(|| v.as_str().and_then(|s| s.parse::<i64>().ok()))).unwrap_or(0);
    let (is_default, count): (i64, i64) = sqlx::query_as(
        "SELECT is_default,(SELECT COUNT(*) FROM role) FROM role WHERE role_id=?",
    )
    .bind(id).fetch_one(&state.pool).await.unwrap_or((0, 0));
    if count == 0 {
        return err(400, "notExist");
    }
    if is_default == 1 {
        return err(400, "delDefRole");
    }
    let default_role: Option<i64> = sqlx::query_scalar("SELECT role_id FROM role WHERE is_default=1 LIMIT 1")
        .fetch_optional(&state.pool).await.ok().flatten();
    if let Some(def) = default_role {
        let _ = sqlx::query("UPDATE user SET type=? WHERE type=?").bind(def).bind(id).execute(&state.pool).await;
    }
    let _ = sqlx::query("DELETE FROM role_perm WHERE role_id=?").bind(id).execute(&state.pool).await;
    let _ = sqlx::query("DELETE FROM role WHERE role_id=?").bind(id).execute(&state.pool).await;
    ok_value(json!(null))
}

fn join(values: &[Value]) -> String {
    values.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>().join(",")
}

fn split(raw: &str) -> Vec<String> {
    raw.split(',').filter(|v| !v.is_empty()).map(|v| v.to_string()).collect()
}
