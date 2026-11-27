use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use sqlx::Column;
use axum::Json;
use base64::Engine;
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::sync::Arc;

use crate::handlers::{ok_value, AuthUser};
use crate::models::AppState;

#[derive(Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
    #[serde(default)]
    pub token: Option<String>,
}

#[derive(Deserialize)]
pub struct RegisterRequest {
    pub email: String,
    pub password: String,
    pub token: Option<String>,
    pub code: Option<String>,
}

/// 登录失败锁定阈值与时长：连续失败 5 次锁定 15 分钟（IP + 邮箱双维度）。
const LOGIN_MAX_FAIL: i64 = 5;
const LOGIN_LOCK_SECS: i64 = 15 * 60;
/// JWT 有效期 24 小时。
const JWT_TTL_SECS: i64 = 24 * 3600;

pub async fn login(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<LoginRequest>,
) -> Response {
    if req.email.is_empty() || req.password.is_empty() {
        return err(400, "emailAndPwdEmpty");
    }
    let ip = crate::utils::client_ip(&headers, "Unknown");

    // 防爆破：IP 与邮箱任一处于锁定窗口内即拒绝。
    if is_locked(&state, &ip, &req.email).await {
        return err(423, "loginLocked");
    }

    // Turnstile：OPEN(0) 每次都验证；COUNT(2) 在免费次数用尽后验证
    // （原版 login-service.js: verifyRecordService.consumeLoginAttempt）。
    let setting = current_setting(&state).await;
    let login_verify = setting["login_verify"].as_i64().unwrap_or(1);
    let login_verify_count = setting["login_verify_count"].as_i64().unwrap_or(1);
    let need_verify = login_verify == 0
        || (login_verify == 2 && !super::verify::consume_login_attempt(&state, &ip, login_verify_count).await);
    if need_verify {
        let token = req.token.as_deref().unwrap_or("");
        if let Err(msg) = super::verify::verify_turnstile(&state, token, &ip).await {
            return err(400, &msg);
        }
    }

    // 账号枚举防护：不存在 / 已删 / 封禁 / 密码错统一返回 IncorrectPwd。
    let user: Option<(i64, String, String, String, i64, i64)> = sqlx::query_as(
        "SELECT user_id,email,password,salt,status,is_del FROM user WHERE email = ? COLLATE NOCASE"
    )
    .bind(&req.email)
    .fetch_optional(&state.pool)
    .await
    .unwrap();
    let valid = match user {
        Some((user_id, _, stored, salt, status, is_del))
            if is_del == 0 && status == 0 && verify_password(&req.password, &salt, &stored) =>
        {
            Some(user_id)
        }
        _ => None,
    };
    let user_id = match valid {
        Some(id) => id,
        None => {
            record_login_failure(&state, &ip, &req.email).await;
            return err(400, "IncorrectPwd");
        }
    };

    // 登录成功：清零该 IP+邮箱的失败计数与锁定。
    clear_login_failure(&state, &ip, &req.email).await;
    let token = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO auth_session(token,user_id) VALUES(?,?)")
        .bind(&token)
        .bind(user_id)
        .execute(&state.pool)
        .await
        .unwrap();
    trim_sessions(&state.pool, user_id).await;
    // JWT 带 exp（24h）；exp 校验在 AuthUser 提取层强制开启。
    let jwt = match crate::utils::jwt_sign(
        &json!({"user_id": user_id, "token": token, "gen": crate::handlers::AUTH_GEN}),
        &state.config.jwt_secret,
        JWT_TTL_SECS,
    ) {
        Ok(jwt) => jwt,
        Err(e) => return err(500, &e),
    };
    ok_value(json!({ "token": jwt }))
}

/// 检查 IP 或邮箱是否被锁定；locked_until 为 UTC 的 "YYYY-MM-DD HH:MM:SS"。
async fn is_locked(state: &Arc<AppState>, ip: &str, email: &str) -> bool {
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let ip_locked: Option<String> = sqlx::query_scalar(
        "SELECT locked_until FROM login_attempt WHERE ip=? AND locked_until IS NOT NULL AND locked_until>?",
    )
    .bind(ip).bind(&now).fetch_one(&state.pool).await.ok().flatten();
    if ip_locked.is_some() {
        return true;
    }
    let email_locked: Option<String> = sqlx::query_scalar(
        "SELECT locked_until FROM login_attempt WHERE email=? COLLATE NOCASE AND locked_until IS NOT NULL AND locked_until>?",
    )
    .bind(email).bind(&now).fetch_one(&state.pool).await.ok().flatten();
    email_locked.is_some()
}

/// 记录一次失败：对 IP 维度（email=''）与邮箱维度（ip=''）各 +1；
/// 达到阈值则设置锁定窗口。
async fn record_login_failure(state: &Arc<AppState>, ip: &str, email: &str) {
    let bump = format!(
        "INSERT INTO login_attempt(ip,email,fail_count) VALUES(?,?,1) \
         ON CONFLICT(ip,email) DO UPDATE SET fail_count=fail_count+1, \
         locked_until=CASE WHEN fail_count+1>={} THEN datetime('now','+{} seconds') ELSE locked_until END",
        LOGIN_MAX_FAIL, LOGIN_LOCK_SECS,
    );
    let _ = sqlx::query(&bump).bind(ip).bind("").execute(&state.pool).await;
    let _ = sqlx::query(&bump).bind("").bind(email).execute(&state.pool).await;
}

/// 登录成功后清除失败计数（按本次 IP 与邮箱）。
async fn clear_login_failure(state: &Arc<AppState>, ip: &str, email: &str) {
    let _ = sqlx::query("DELETE FROM login_attempt WHERE ip=? OR email=? COLLATE NOCASE")
        .bind(ip).bind(email).execute(&state.pool).await;
}

pub async fn register(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<RegisterRequest>,
) -> Response {
    // 注册门禁：仅允许配置文件中的 admin 邮箱注册；该账号一旦存在，注册整体关闭。
    // 优先于 setting.register 开关与注册码等策略，等价于「一次性初始化管理员」。
    let admin_email = state.config.admin.as_str();
    if !req.email.eq_ignore_ascii_case(admin_email) {
        return err(400, "regDisabled");
    }
    let admin_exists: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM user WHERE email=? COLLATE NOCASE")
            .bind(admin_email)
            .fetch_one(&state.pool)
            .await
            .unwrap_or(0);
    if admin_exists > 0 {
        return err(400, "regDisabled");
    }

    let setting = current_setting(&state).await;
    if setting["register"].as_i64().unwrap_or(0) == 1 {
        return err(400, "regDisabled");
    }
    if !valid_email(&req.email) {
        return err(400, "notEmail");
    }
    // Email prefix (local part) rules from settings.
    let local = req.email.split('@').next().unwrap_or("");
    let min_prefix = setting["min_email_prefix"].as_i64().unwrap_or(1).max(0) as usize;
    if local.chars().count() < min_prefix {
        return err(400, "minEmailPrefix");
    }
    if local.chars().count() > 64 {
        return err(400, "emailLengthLimit");
    }
    let prefix_filter = setting["email_prefix_filter"].as_str().unwrap_or("");
    if prefix_filter
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .any(|s| local.to_lowercase().contains(&s.to_lowercase()))
    {
        return err(400, "banEmailPrefix");
    }
    if req.password.len() < 6 || req.password.len() > 30 {
        return err(400, "pwdMinLength");
    }
    if !state.config.domain_allowed(&req.email) {
        return err(400, "notEmailDomain");
    }

    // Turnstile（原版 registerVerify OPEN/COUNT 两种触发方式）
    let ip = crate::utils::client_ip(&headers, "Unknown");
    let register_verify = setting["register_verify"].as_i64().unwrap_or(1);
    let reg_verify_count = setting["reg_verify_count"].as_i64().unwrap_or(1);
    let mut reg_verify_open = false;
    match register_verify {
        0 => {
            reg_verify_open = true;
            let token = req.token.as_deref().unwrap_or("");
            if let Err(msg) = super::verify::verify_turnstile(&state, token, &ip).await {
                return err(400, &msg);
            }
        }
        2 => {
            reg_verify_open =
                super::verify::is_open_reg_verify(&state, &ip, reg_verify_count).await;
            if reg_verify_open {
                let token = req.token.as_deref().unwrap_or("");
                if let Err(msg) = super::verify::verify_turnstile(&state, token, &ip).await {
                    return err(400, &msg);
                }
            }
        }
        _ => {}
    }

    let (role_id, used_code) = match resolve_role_for_register(&state, req.code.as_deref()).await {
        Ok(v) => v,
        Err(msg) => return err(400, &msg),
    };

    // 角色可用域名权限（原版 roleService.hasAvailDomainPerm）
    let avail_domain: Option<String> =
        sqlx::query_scalar("SELECT avail_domain FROM role WHERE role_id=?")
            .bind(role_id)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None);
    if let Some(avail) = avail_domain {
        if !super::common::has_avail_domain_perm(&avail, &req.email) {
            return err(400, if used_code { "noDomainPermRegKey" } else { "noDomainPermReg" });
        }
    }

    // 原版 loginService.register 会把使用的注册码 id 写入 user.regKeyId；
    // /regKey/history 依赖该关联，此前 Rust 恒写 0。
    let reg_key_id: i64 = if used_code {
        match req.code.as_deref() {
            Some(code) => {
                sqlx::query_scalar("SELECT rege_key_id FROM reg_key WHERE code=? COLLATE NOCASE")
                    .bind(code)
                    .fetch_optional(&state.pool)
                    .await
                    .ok()
                    .flatten()
                    .unwrap_or(0)
            }
            None => 0,
        }
    } else {
        0
    };
    let (salt, hash) = hash_password(&req.password);
    let result = sqlx::query(
        "INSERT INTO user(email,type,password,salt,status,reg_key_id) VALUES(?,?,?,?,0,?)"
    )
    .bind(&req.email)
    .bind(role_id)
    .bind(&hash)
    .bind(&salt)
    .bind(reg_key_id)
    .execute(&state.pool)
    .await;
    if let Err(e) = result {
        tracing::error!("register failed: {e}");
        return err(500, "registerFailed");
    }
    let user_id = result.unwrap().last_insert_rowid();
    let name = req.email.split('@').next().unwrap_or("").to_string();
    sqlx::query("INSERT INTO account(email,name,user_id) VALUES(?,?,?)")
        .bind(&req.email)
        .bind(&name)
        .bind(user_id)
        .execute(&state.pool)
        .await
        .unwrap();

    // COUNT 模式：本次未触发验证则计数 +1，返回是否已达验证阈值
    if register_verify == 2 && !reg_verify_open {
        let count = super::verify::increase_reg_count(&state, &ip).await;
        reg_verify_open = count >= reg_verify_count;
    }
    ok_value(json!({ "regVerifyOpen": reg_verify_open }))
}

fn default_role() -> i64 {
    1
}

/// Resolve the role for a new registration, honouring the `reg_key` mode.
///
/// Mirrors `loginService.register` from the original Worker:
///   * regKey == 0 (OPEN)    → a valid, unconsumed, unexpired code is required.
///   * regKey == 2 (OPTIONAL)→ a valid code upgrades the role, otherwise default.
///   * otherwise (CLOSE)     → default role, ignore any code.
///
/// A successfully used code has its remaining count decremented. Returns
/// `(role_id, used_code)` — `used_code` 区分注册码角色与默认角色，供域名权限
/// 校验选择错误码。错误时返回 i18n 风格的错误消息键。
pub async fn resolve_role_for_register(
    state: &AppState,
    code: Option<&str>,
) -> Result<(i64, bool), String> {
    let setting = current_setting(state).await;
    let reg_key = setting.get("reg_key").and_then(|v| v.as_i64()).unwrap_or(1);
    let code = code.filter(|c| !c.is_empty());

    // If the mode may use a code, resolve its role id. `None` means the code is
    // invalid / exhausted / expired.
    let resolved: Option<(String, i64)> = match code {
        Some(code) => {
            let row = sqlx::query_as::<_, (i64, i64, Option<String>)>(
                "SELECT role_id,count,expire_time FROM reg_key WHERE code=? COLLATE NOCASE \
                 ORDER BY rege_key_id DESC LIMIT 1",
            )
            .bind(code)
            .fetch_optional(&state.pool)
            .await
            .ok()
            .flatten();
            match row {
                Some((role_id, count, expire)) if count > 0 => {
                    let still_valid = match expire.as_deref() {
                        Some(e) => sqlx::query_scalar::<_, i64>(
                            "SELECT datetime(?) > datetime('now')",
                        )
                        .bind(e)
                        .fetch_one(&state.pool)
                        .await
                        .unwrap_or(0)
                            == 1,
                        None => true,
                    };
                    if still_valid {
                        Some((code.to_string(), role_id))
                    } else {
                        None
                    }
                }
                _ => None,
            }
        }
        None => None,
    };

    match reg_key {
        0 => {
            // OPEN: code mandatory
            let code = code.ok_or_else(|| "emptyRegKey".to_string())?;
            let (_, role_id) = resolved.ok_or_else(|| "noRegKeyCount".to_string())?;
            let _ = sqlx::query("UPDATE reg_key SET count=count-1 WHERE code=? COLLATE NOCASE")
                .bind(code)
                .execute(&state.pool)
                .await;
            Ok((role_id, true))
        }
        2 => {
            // OPTIONAL: default unless a valid code upgrades the role.
            if let Some((code, role_id)) = resolved {
                let _ = sqlx::query("UPDATE reg_key SET count=count-1 WHERE code=? COLLATE NOCASE")
                    .bind(&code)
                    .execute(&state.pool)
                    .await;
                return Ok((role_id, true));
            }
            Ok((default_role(), false))
        }
        _ => Ok((default_role(), false)),
    }
}

pub async fn logout(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
) -> Response {
    let _ = sqlx::query("DELETE FROM auth_session WHERE token=? AND user_id=?")
        .bind(&user.token)
        .bind(user.user_id)
        .execute(&state.pool)
        .await;
    ok_value(json!(null))
}

pub async fn login_user_info(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
) -> Response {
    let row: Option<(i64, String, i64, i64, i64)> = sqlx::query_as(
        "SELECT user_id,email,type,send_count,is_del FROM user WHERE user_id=?"
    )
    .bind(user.user_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap();
    let (user_id, email, type_id, send_count, _) = match row {
        Some(row) => row,
        None => return err(401, "authExpired"),
    };
    let account: Option<(i64, String, String, i64)> = sqlx::query_as(
        "SELECT account_id,email,name,all_receive FROM account WHERE email=? COLLATE NOCASE AND is_del=0 LIMIT 1"
    ).bind(&email).fetch_optional(&state.pool).await.unwrap();
    let role: Option<(i64, String, String, i64)> = sqlx::query_as(
        "SELECT role_id,name,send_type,send_count FROM role WHERE role_id=?"
    ).bind(type_id).fetch_optional(&state.pool).await.unwrap();
    let is_admin = email.eq_ignore_ascii_case(&state.config.admin);
    let perm_keys = if is_admin {
        vec!["*".to_string()]
    } else {
        perm_keys(&state.pool, user_id).await
    };
    let role_json = if is_admin {
        json!({"name":"admin","sendCount":0,"sendType":"count","accountCount":0})
    } else {
        match role {
            Some((id, name, send_type, send_count)) => json!({
                "roleId": id, "name": name, "sendType": send_type, "sendCount": send_count
            }),
            None => json!(null),
        }
    };
    ok_value(json!({
        "userId": user_id,
        "email": email,
        "sendCount": send_count,
        "type": if is_admin {0} else {type_id},
        "account": account.as_ref().map(|a| json!({"accountId":a.0,"email":a.1,"name":a.2,"allReceive":a.3})).unwrap_or(json!(null)),
        "name": account.as_ref().map(|a| a.2.clone()).unwrap_or_default(),
        "role": role_json,
        "permKeys": perm_keys
    }))
}

pub async fn reset_password(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(req): Json<RegisterRequest>,
) -> Response {
    if req.password.len() < 6 {
        return err(400, "pwdMinLength");
    }
    let (salt, hash) = hash_password(&req.password);
    sqlx::query("UPDATE user SET password=?,salt=? WHERE user_id=?")
        .bind(hash)
        .bind(salt)
        .bind(user.user_id)
        .execute(&state.pool)
        .await
        .unwrap();
    ok_value(json!(null))
}

pub async fn delete_account(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
) -> Response {
    sqlx::query("UPDATE user SET is_del=1 WHERE user_id=?")
        .bind(user.user_id)
        .execute(&state.pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM auth_session WHERE user_id=?")
        .bind(user.user_id)
        .execute(&state.pool)
        .await
        .unwrap();
    ok_value(json!(null))
}

pub async fn current_setting(state: &AppState) -> serde_json::Value {
    sqlx::query("SELECT * FROM setting WHERE id=1")
        .fetch_optional(&state.pool)
        .await
        .ok()
        .flatten()
        .map(row_to_setting)
        .unwrap_or_else(|| json!({"register":0,"reg_key":1}))
}

fn row_to_setting(row: sqlx::sqlite::SqliteRow) -> serde_json::Value {
    use sqlx::Row;
    let mut map = serde_json::Map::new();
    for column in row.columns() {
        let name = column.name();
        let value: serde_json::Value = match row.try_get::<Option<String>, _>(name) {
            Ok(Some(v)) => json!(v),
            Ok(None) => json!(null),
            Err(_) => match row.try_get::<Option<i64>, _>(name) {
                Ok(Some(v)) => json!(v),
                Ok(None) => json!(null),
                Err(_) => match row.try_get::<Option<f64>, _>(name) {
                    Ok(Some(v)) => json!(v),
                    _ => json!(null),
                },
            },
        };
        map.insert(name.to_string(), value);
    }
    serde_json::Value::Object(map)
}

async fn perm_keys(pool: &sqlx::SqlitePool, user_id: i64) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT DISTINCT p.perm_key FROM perm p JOIN role_perm rp ON rp.perm_id=p.perm_id JOIN user u ON u.type=rp.role_id WHERE u.user_id=? AND p.perm_key IS NOT NULL AND p.perm_key <> ''"
    ).bind(user_id).fetch_all(pool).await.unwrap_or_default()
}

pub async fn trim_sessions(pool: &sqlx::SqlitePool, user_id: i64) {
    let tokens: Vec<(String,)> = sqlx::query_as("SELECT token FROM auth_session WHERE user_id=? ORDER BY created_at DESC")
        .bind(user_id)
        .fetch_all(pool)
        .await
        .unwrap_or_default();
    if tokens.len() > 10 {
        for (token,) in tokens.iter().skip(10) {
            let _ = sqlx::query("DELETE FROM auth_session WHERE token=?")
                .bind(token)
                .execute(pool)
                .await;
        }
    }
}

/// 密码哈希：argon2id（PHC 字符串），弱随机兜底保留旧 SHA256+salt 逻辑。
/// 返回 (salt, hash)：argon2 路径 salt 留空（已内含于 PHC），hash 为 PHC 字符串。
pub fn hash_password(password: &str) -> (String, String) {
    use argon2::password_hash::{rand_core::OsRng, SaltString};
    use argon2::{Argon2, PasswordHasher};
    let salt = SaltString::generate(&mut OsRng);
    match Argon2::default().hash_password(password.as_bytes(), &salt) {
        Ok(phc) => (String::new(), phc.to_string()),
        Err(_) => legacy_hash_password(password),
    }
}

/// 校验密码：PHC（argon2id）优先，旧 SHA256+salt 自动兼容，避免存量用户失效。
pub fn verify_password(password: &str, salt: &str, stored: &str) -> bool {
    use argon2::{Argon2, PasswordHash, PasswordVerifier};
    if stored.starts_with("$argon2") {
        return PasswordHash::new(stored)
            .map(|parsed| Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok())
            .unwrap_or(false);
    }
    let mut hasher = Sha256::new();
    hasher.update(salt.as_bytes());
    hasher.update(password.as_bytes());
    base64::engine::general_purpose::STANDARD.encode(hasher.finalize()) == stored
}

fn legacy_hash_password(password: &str) -> (String, String) {
    let mut salt = [0u8; 16];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut salt);
    let salt = base64::engine::general_purpose::STANDARD.encode(salt);
    let mut hasher = Sha256::new();
    hasher.update(salt.as_bytes());
    hasher.update(password.as_bytes());
    (salt, base64::engine::general_purpose::STANDARD.encode(hasher.finalize()))
}

pub fn valid_email(email: &str) -> bool {
    let parts: Vec<&str> = email.split('@').collect();
    parts.len() == 2 && !parts[0].is_empty() && parts[1].contains('.') && !parts[1].starts_with('.')
}

fn err(code: u16, message: &str) -> Response {
    (
        axum::http::StatusCode::OK,
        Json(json!({"code": code, "message": message})),
    )
        .into_response()
}
