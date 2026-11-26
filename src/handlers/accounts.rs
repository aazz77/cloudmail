use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

use crate::handlers::{ok_value, AuthUser};
use crate::models::AppState;

#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    account_id: Option<i64>,
    #[serde(default)]
    size: Option<i64>,
    #[serde(default)]
    last_sort: Option<i64>,
}

pub async fn list(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Query(q): Query<ListQuery>,
) -> Response {
    let size = q.size.unwrap_or(30).clamp(1, 30);
    let last_sort = q.last_sort.unwrap_or(9_999_999_999);
    let account_id = q.account_id.unwrap_or(0);
    let rows: Vec<(i64, String, String, i64, Option<String>, i64)> = sqlx::query_as(
        "SELECT account_id,email,name,status,latest_email_time,all_receive FROM account WHERE user_id=? AND is_del=0 AND (sort<? OR (sort=? AND account_id>?)) ORDER BY sort DESC, account_id ASC LIMIT ?"
    )
    .bind(user.user_id).bind(last_sort).bind(last_sort).bind(account_id).bind(size)
    .fetch_all(&state.pool).await.unwrap_or_default();
    let data = rows.into_iter().map(|r| json!({
        "accountId": r.0, "email": r.1, "name": r.2, "status": r.3,
        "latestEmailTime": r.4, "allReceive": r.5
    })).collect::<Vec<_>>();
    ok_value(Value::Array(data))
}

#[derive(Deserialize)]
pub struct AddRequest {
    pub email: String,
    #[serde(default)]
    pub token: Option<String>,
}

pub async fn add(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    headers: HeaderMap,
    Json(req): Json<AddRequest>,
) -> Response {
    // 对齐原版 accountService.add：开关 → 格式/域名/前缀 → 归属 → 角色 → Turnstile。
    let setting = crate::handlers::auth::current_setting(&state).await;
    let add_email_verify = setting["add_email_verify"].as_i64().unwrap_or(1);
    let add_email = setting["add_email"].as_i64().unwrap_or(0);
    let many_email = setting["many_email"].as_i64().unwrap_or(0);
    let add_verify_count = setting["add_verify_count"].as_i64().unwrap_or(1);
    let min_email_prefix = setting["min_email_prefix"].as_i64().unwrap_or(0).max(0) as usize;
    let prefix_filter = setting["email_prefix_filter"].as_str().unwrap_or("").to_string();

    // 加邮箱总开关：addEmail 与 manyEmail 都要为 OPEN(0)
    if !(add_email == 0 && many_email == 0) {
        return err(400, "addAccountDisabled");
    }
    if req.email.is_empty() {
        return err(400, "emptyEmail");
    }
    if !crate::utils::valid_email(&req.email) {
        return err(400, "notEmail");
    }
    if !state.config.domain_allowed(&req.email) {
        return err(400, "notExistDomain");
    }
    let name = crate::handlers::common::get_name(&req.email);
    if name.chars().count() < min_email_prefix {
        return err(400, "minEmailPrefix");
    }
    if prefix_filter
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .any(|s| name.to_lowercase().contains(&s.to_lowercase()))
    {
        return err(400, "banEmailPrefix");
    }

    // 已存在的账号（含已删除）
    let existing: Option<(i64, i64)> = sqlx::query_as(
        "SELECT account_id, is_del FROM account WHERE email=? COLLATE NOCASE",
    )
    .bind(&req.email)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);
    if let Some((_, is_del)) = existing {
        return err(400, if is_del == 1 { "isDelAccount" } else { "isRegAccount" });
    }

    // `+` 别名归属：基础地址必须存在且属于当前用户
    if req.email.contains('+') {
        let base = crate::handlers::common::get_base_email(&req.email);
        let base_owner: Option<i64> = sqlx::query_scalar(
            "SELECT user_id FROM account WHERE email=? COLLATE NOCASE",
        )
        .bind(&base)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None);
        match base_owner {
            Some(owner) if owner == user.user_id => {}
            _ => return err(400, "notOwner"),
        }
    }

    // 角色邮箱数量上限与可用域名权限（管理员豁免）
    let is_admin = {
        let email: Option<String> = sqlx::query_scalar("SELECT email FROM user WHERE user_id=?")
            .bind(user.user_id)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None);
        email.map(|e| e.eq_ignore_ascii_case(&state.config.admin)).unwrap_or(false)
    };
    let role: Option<(i64, String, Option<i64>)> = sqlx::query_as(
        "SELECT r.role_id, r.avail_domain, r.account_count FROM user u JOIN role r ON r.role_id=u.type WHERE u.user_id=?",
    )
    .bind(user.user_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);
    let (_role_id, role_avail, role_account_count) = role.unwrap_or((1, String::new(), None));
    if !is_admin {
        if let Some(limit) = role_account_count.filter(|v| *v > 0) {
            let count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM account WHERE user_id=? AND is_del=0",
            )
            .bind(user.user_id)
            .fetch_one(&state.pool)
            .await
            .unwrap_or(0);
            if count >= limit {
                return err(403, "accountLimit");
            }
        }
        if !crate::handlers::common::has_avail_domain_perm(&role_avail, &req.email) {
            return err(403, "noDomainPermAdd");
        }
    }

    // Turnstile（addEmailVerify OPEN/COUNT）
    let ip = crate::utils::client_ip(&headers, "Unknown");
    let mut add_verify_open = false;
    match add_email_verify {
        0 => {
            add_verify_open = true;
            let token = req.token.as_deref().unwrap_or("");
            if let Err(msg) = crate::handlers::verify::verify_turnstile(&state, token, &ip).await {
                return err(400, &msg);
            }
        }
        2 => {
            add_verify_open =
                crate::handlers::verify::is_open_add_verify(&state, &ip, add_verify_count).await;
            if add_verify_open {
                let token = req.token.as_deref().unwrap_or("");
                if let Err(msg) = crate::handlers::verify::verify_turnstile(&state, token, &ip).await {
                    return err(400, &msg);
                }
            }
        }
        _ => {}
    }

    let result = sqlx::query("INSERT INTO account(email,name,user_id) VALUES(?,?,?)")
        .bind(&req.email).bind(&name).bind(user.user_id)
        .execute(&state.pool).await;
    match result {
        Ok(r) => {
            // COUNT 模式：本次未验证则计数 +1，返回是否已达阈值
            if add_email_verify == 2 && !add_verify_open {
                let count = crate::handlers::verify::increase_add_count(&state, &ip).await;
                add_verify_open = count >= add_verify_count;
            }
            ok_value(json!({
                "accountId": r.last_insert_rowid(), "email": req.email,
                "name": name, "addVerifyOpen": add_verify_open
            }))
        }
        Err(e) => { tracing::error!("add account failed: {e}"); err(500, "addAccountFailed") }
    }
}

#[derive(Deserialize)]
pub struct IdRequest {
    #[serde(alias = "accountId")]
    pub account_id: i64,
    #[serde(default)]
    pub name: Option<String>,
}

pub async fn set_name(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(req): Json<IdRequest>,
) -> Response {
    if let Some(name) = &req.name {
        if name.len() > 30 {
            return err(400, "usernameLengthLimit");
        }
        sqlx::query("UPDATE account SET name=? WHERE account_id=? AND user_id=?")
            .bind(name).bind(req.account_id).bind(user.user_id)
            .execute(&state.pool).await.unwrap();
    }
    ok_value(json!(null))
}

pub async fn delete(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Query(q): Query<IdRequest>,
) -> Response {
    sqlx::query("UPDATE account SET is_del=1 WHERE account_id=? AND user_id=?")
        .bind(q.account_id).bind(user.user_id)
        .execute(&state.pool).await.unwrap();
    ok_value(json!(null))
}

pub async fn set_all_receive(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(req): Json<IdRequest>,
) -> Response {
    sqlx::query("UPDATE account SET all_receive=1-all_receive WHERE account_id=? AND user_id=?")
        .bind(req.account_id).bind(user.user_id)
        .execute(&state.pool).await.unwrap();
    ok_value(json!(null))
}

pub async fn set_as_top(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(req): Json<IdRequest>,
) -> Response {
    sqlx::query("UPDATE account SET sort=(SELECT COALESCE(MAX(sort),0)+1 FROM account) WHERE account_id=? AND user_id=?")
        .bind(req.account_id).bind(user.user_id)
        .execute(&state.pool).await.unwrap();
    ok_value(json!(null))
}

fn err(code: u16, message: &str) -> Response {
    (axum::http::StatusCode::OK, Json(json!({"code":code,"message":message}))).into_response()
}
