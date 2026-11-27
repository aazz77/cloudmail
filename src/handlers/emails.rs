use axum::extract::{Query, State};
use axum::response::{IntoResponse, Response};
use axum::Json;
use sqlx::Row;
use std::sync::Arc;
use crate::models::AppState;
use base64::Engine;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use chrono::Datelike;

use crate::handlers::{ok_value, AuthUser};

#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default, alias = "emailId")] pub email_id: Option<i64>,
    #[serde(default, alias = "accountId")] pub account_id: Option<i64>,
    #[serde(default)] pub size: Option<i64>,
    #[serde(default, alias = "timeSort")] pub time_sort: Option<i64>,
    #[serde(default, alias = "allReceive")] pub all_receive: Option<i64>,
    #[serde(default)] pub full: Option<i64>,
    #[serde(default)] pub r#type: Option<i64>,
    #[serde(default, alias = "searchType")] pub search_type: Option<String>,
    #[serde(default)] pub text: Option<String>,
    #[serde(default, alias = "startDate")] pub start_date: Option<String>,
    #[serde(default, alias = "endDate")] pub end_date: Option<String>,
}

/// Build the WHERE clause + ordered binds shared by the list query, the total
/// count query, and (partly) the latest-email query.
struct ListFilters {
    where_sql: String,
    binds: Vec<FilterBind>,
}

#[derive(Clone)]
enum FilterBind {
    Text(String),
    Num(i64),
}

fn build_list_filters(q: &ListQuery, user: &AuthUser, with_cursor: bool) -> ListFilters {
    let typ = q.r#type.unwrap_or(0);
    let account_id = q.account_id.unwrap_or(0);
    let all_receive = q.all_receive.unwrap_or(0);
    let time_sort = q.time_sort.unwrap_or(0);

    let mut parts: Vec<String> = Vec::new();
    let mut binds: Vec<FilterBind> = Vec::new();

    parts.push("e.user_id=?".into());
    binds.push(FilterBind::Num(user.user_id));
    parts.push("e.type=?".into());
    binds.push(FilterBind::Num(typ));
    parts.push("e.is_del=0".into());
    parts.push("a.is_del=0".into());
    if all_receive == 0 {
        parts.push("e.account_id=?".into());
        binds.push(FilterBind::Num(account_id));
    }
    if with_cursor {
        if let Some(cursor) = q.email_id.filter(|v| *v > 0) {
            parts.push(if time_sort == 1 {
                "e.email_id>?"
            } else {
                "e.email_id<?"
            }
            .into());
            binds.push(FilterBind::Num(cursor));
        }
    }

    // Text search by searchType — mirrors emailListFilters.
    if let Some(text) = q.text.as_ref().map(|t| t.trim()).filter(|t| !t.is_empty()) {
        let pattern = format!("%{}%", text);
        match q.search_type.as_deref().unwrap_or("") {
            "subject" => {
                parts.push("e.subject COLLATE NOCASE LIKE ?".into());
                binds.push(FilterBind::Text(pattern));
            }
            "content" => {
                parts.push("(e.text COLLATE NOCASE LIKE ? OR e.content COLLATE NOCASE LIKE ?)".into());
                binds.push(FilterBind::Text(pattern.clone()));
                binds.push(FilterBind::Text(pattern));
            }
            "sender" => {
                parts.push("e.name COLLATE NOCASE LIKE ?".into());
                binds.push(FilterBind::Text(pattern));
            }
            "senderAddress" => {
                parts.push("e.send_email COLLATE NOCASE LIKE ?".into());
                binds.push(FilterBind::Text(pattern));
            }
            "recipientAddress" => {
                parts.push("e.to_email COLLATE NOCASE LIKE ?".into());
                binds.push(FilterBind::Text(pattern));
            }
            _ => {}
        }
    }

    // Date range filter (scoped to this user).
    let has_start = q.start_date.as_ref().map(|s| !s.is_empty()).unwrap_or(false);
    let has_end = q.end_date.as_ref().map(|s| !s.is_empty()).unwrap_or(false);
    if has_start || has_end {
        let mut sub = vec!["user_id=?".to_string()];
        let mut sub_binds: Vec<FilterBind> = vec![FilterBind::Num(user.user_id)];
        if let Some(s) = q.start_date.as_ref().filter(|s| !s.is_empty()) {
            sub.push("create_time>=?".into());
            sub_binds.push(FilterBind::Text(s.clone()));
        }
        if let Some(e) = q.end_date.as_ref().filter(|s| !s.is_empty()) {
            sub.push("create_time<=?".into());
            sub_binds.push(FilterBind::Text(e.clone()));
        }
        parts.push(format!(
            "e.email_id IN (SELECT email_id FROM email WHERE {})",
            sub.join(" AND ")
        ));
        binds.extend(sub_binds);
    }

    ListFilters {
        where_sql: parts.join(" AND "),
        binds,
    }
}

pub async fn list(State(state): State<Arc<AppState>>, user: AuthUser, Query(q): Query<ListQuery>) -> Response {
    use sqlx::Row;
    let size = q.size.unwrap_or(10).clamp(1, 50);
    let time_sort = q.time_sort.unwrap_or(0);
    let full = q.full.unwrap_or(1) == 1;
    let order = if time_sort == 1 { "ASC" } else { "DESC" };

    let filters = build_list_filters(&q, &user, true);

    // Main list query: join account (for is_del) and left-join star.
    let columns = if full {
        "e.*, s.star_id"
    } else {
        "e.email_id,e.send_email,e.name,e.subject,e.code,e.recipient,e.cc,e.to_email,e.type,e.status,e.message,e.unread,e.create_time,e.is_del,e.text,e.content,s.star_id"
    };
    let list_sql = format!(
        "SELECT {columns} FROM email AS e \
         INNER JOIN account AS a ON a.account_id=e.account_id \
         LEFT JOIN star AS s ON s.email_id=e.email_id AND s.user_id=? \
         WHERE {where_sql} ORDER BY e.email_id {order} LIMIT ?",
        where_sql = filters.where_sql,
    );
    let mut list_query = sqlx::query(&list_sql).bind(user.user_id);
    for b in &filters.binds {
        list_query = match b {
            FilterBind::Text(v) => list_query.bind(v.clone()),
            FilterBind::Num(n) => list_query.bind(*n),
        };
    }
    let rows = list_query.bind(size).fetch_all(&state.pool).await.unwrap_or_default();

    // Total count (no cursor).
    let count_filters = build_list_filters(&q, &user, false);
    let count_sql = format!(
        "SELECT COUNT(*) FROM email AS e \
         INNER JOIN account AS a ON a.account_id=e.account_id \
         WHERE {}",
        count_filters.where_sql
    );
    let mut count_query = sqlx::query_scalar::<_, i64>(&count_sql);
    for b in &count_filters.binds {
        count_query = match b {
            FilterBind::Text(v) => count_query.bind(v.clone()),
            FilterBind::Num(n) => count_query.bind(*n),
        };
    }
    let total = count_query.fetch_one(&state.pool).await.unwrap_or(0);

    // Latest email in this view (no cursor) for the client's sync anchor.
    let typ = q.r#type.unwrap_or(0);
    let account_id = q.account_id.unwrap_or(0);
    let all_receive = q.all_receive.unwrap_or(0);
    let mut latest_sql = String::from(
        "SELECT email_id,account_id,user_id FROM email WHERE user_id=? AND type=? AND is_del=0",
    );
    if all_receive == 0 {
        latest_sql.push_str(" AND account_id=?");
    }
    latest_sql.push_str(" ORDER BY email_id DESC LIMIT 1");
    let mut latest_q = sqlx::query_as::<_, (i64, i64, i64)>(&latest_sql)
        .bind(user.user_id)
        .bind(typ);
    if all_receive == 0 {
        latest_q = latest_q.bind(account_id);
    }
    let latest_row = latest_q.fetch_optional(&state.pool).await.ok().flatten();
    let latest_email = latest_row
        .map(|r| json!({"emailId": r.0, "accountId": r.1, "userId": r.2}))
        .unwrap_or(json!({"emailId": 0, "accountId": account_id, "userId": user.user_id}));

    // Convert rows to JSON (camelCase, matching the Drizzle entity contract),
    // adding isStar and (for full) attList.
    let mut list: Vec<Value> = Vec::new();
    for row in &rows {
        let mut obj = crate::handlers::common::row_json_camel(row).as_object().cloned().unwrap_or_default();
        obj.remove("starId");
        let star_id: Option<i64> = row.try_get("star_id").ok().flatten();
        obj.insert("isStar".into(), json!(if star_id.is_some() { 1 } else { 0 }));
        if full {
            let email_id: i64 = row.get("email_id");
            let atts = sqlx::query(
                "SELECT att_id,user_id,email_id,account_id,key,filename,mime_type,size,type \
                 FROM attachments WHERE email_id=? AND type=0 ORDER BY att_id",
            )
            .bind(email_id)
            .fetch_all(&state.pool)
            .await
            .unwrap_or_default();
            obj.insert("attList".into(), Value::Array(atts.into_iter().map(att_json).collect()));
        }
        list.push(Value::Object(obj));
    }
    if full {
        crate::handlers::common::rewrite_list_content(&state, &mut list).await;
    }

    ok_value(json!({"list": list, "total": total, "latestEmail": latest_email}))
}

pub async fn latest(State(state): State<Arc<AppState>>, user: AuthUser, Query(q): Query<ListQuery>) -> Response {
    // Original: return received emails newer than `emailId`, for this account
    // (unless allReceive), with attachments and a compact listText.
    let email_id = q.email_id.unwrap_or(0);
    let account_id = q.account_id.unwrap_or(0);
    let all_receive = q.all_receive.unwrap_or(0);

    let mut sql = String::from(
        "SELECT e.* FROM email AS e \
         INNER JOIN account AS a ON a.account_id=e.account_id \
         WHERE e.email_id>? AND e.user_id=? AND e.is_del=0 AND a.is_del=0 AND e.type=0",
    );
    if all_receive == 0 {
        sql.push_str(" AND e.account_id=?");
    }
    sql.push_str(" ORDER BY e.email_id DESC LIMIT 50");

    let mut query = sqlx::query(&sql).bind(email_id).bind(user.user_id);
    if all_receive == 0 {
        query = query.bind(account_id);
    }
    let rows = query.fetch_all(&state.pool).await.unwrap_or_default();

    let mut list: Vec<Value> = Vec::new();
    for row in &rows {
        let mut obj = crate::handlers::common::row_json_camel(row).as_object().cloned().unwrap_or_default();
        let id: i64 = row.get("email_id");
        let atts = sqlx::query(
            "SELECT att_id,user_id,email_id,account_id,key,filename,mime_type,size,type \
             FROM attachments WHERE email_id=? AND type=0 ORDER BY att_id",
        )
        .bind(id)
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default();
        obj.insert("attList".into(), Value::Array(atts.into_iter().map(att_json).collect()));
        obj.insert("isStar".into(), json!(0));
        // compact preview text (300 chars max), whitespace collapsed
        let raw = obj.get("text").and_then(|v| v.as_str()).unwrap_or("").to_string();
        obj.insert("listText".into(), json!(crate::handlers::common::to_list_text(&raw)));
        list.push(Value::Object(obj));
    }
    crate::handlers::common::rewrite_list_content(&state, &mut list).await;
    ok_value(Value::Array(list))
}

#[derive(Deserialize, Default)]
pub struct IdsQuery { #[serde(default, rename = "emailIds")] pub email_ids: Option<String> }

pub async fn delete(State(state): State<Arc<AppState>>, user: AuthUser, Query(q): Query<IdsQuery>) -> Response {
    let ids = ids_from(q.email_ids.unwrap_or_default());
    if ids.is_empty() { return ok_value(json!(null)); }
    let setting = crate::handlers::auth::current_setting(&state).await;
    let sync_delete = setting["sync_delete"].as_i64().unwrap_or(0) == 0; // 0 = OPEN
    if sync_delete {
        // Only physically delete emails owned by this user.
        let owned: Vec<i64> = {
            let ph = vec!["?"; ids.len()].join(",");
            let sql = format!("SELECT email_id FROM email WHERE user_id=? AND email_id IN ({ph})");
            let mut query = sqlx::query_scalar::<_, i64>(&sql).bind(user.user_id);
            for id in &ids { query = query.bind(*id); }
            query.fetch_all(&state.pool).await.unwrap_or_default()
        };
        crate::handlers::tasks::delete_emails_by_ids(&state.pool, &owned).await;
        return ok_value(json!(null));
    }
    let placeholders = vec!["?"; ids.len()].join(",");
    let sql = format!("UPDATE email SET is_del=1 WHERE user_id=? AND email_id IN ({placeholders})");
    let mut query = sqlx::query(&sql).bind(user.user_id);
    for id in &ids { query = query.bind(*id); }
    let _ = query.execute(&state.pool).await;
    ok_value(json!(null))
}

pub async fn read(State(state): State<Arc<AppState>>, user: AuthUser, Json(req): Json<IdsQuery>) -> Response {
    let ids = ids_from(req.email_ids.unwrap_or_default());
    let placeholders = vec!["?"; ids.len()].join(",");
    let sql = format!("UPDATE email SET unread=1 WHERE user_id=? AND email_id IN ({placeholders})");
    let query = sqlx::query(&sql).bind(user.user_id);
    let query = ids.iter().fold(query, |q,id| q.bind(*id));
    let _ = query.execute(&state.pool).await;
    ok_value(json!(null))
}

pub async fn att_list(State(state): State<Arc<AppState>>, user: AuthUser, Query(q): Query<IdsQuery>) -> Response {
    let ids = ids_from(q.email_ids.unwrap_or_default());
    let placeholders = vec!["?"; ids.len()].join(",");
    let sql = format!("SELECT att_id,user_id,email_id,account_id,key,filename,mime_type,size,type FROM attachments WHERE user_id=? AND email_id IN ({placeholders}) AND type=0");
    let query = sqlx::query(&sql).bind(user.user_id);
    let query = ids.iter().fold(query, |q,id| q.bind(*id));
    let rows = query.fetch_all(&state.pool).await.unwrap_or_default();
    ok_value(Value::Array(rows.into_iter().map(att_json).collect()))
}

fn att_json(row: sqlx::sqlite::SqliteRow) -> Value {
    use sqlx::Row;
    json!({
        "attId": row.get::<i64,_>("att_id"), "userId": row.get::<i64,_>("user_id"),
        "emailId": row.get::<i64,_>("email_id"), "accountId": row.get::<i64,_>("account_id"),
        "key": row.get::<String,_>("key"), "filename": row.get::<Option<String>,_>("filename"),
        "mimeType": row.get::<Option<String>,_>("mime_type"), "size": row.get::<Option<i64>,_>("size"),
        "type": row.get::<i64,_>("type")
    })
}

#[derive(Deserialize)]
pub struct SendRequest {
    #[serde(default, alias = "accountId")] pub account_id: i64,
    #[serde(default)] pub name: Option<String>,
    #[serde(default, alias = "sendType")] pub send_type: Option<String>,
    #[serde(default, alias = "emailId")] pub email_id: i64,
    #[serde(default, rename = "receiveEmail")] pub receive_email: Vec<String>,
    #[serde(default, rename = "ccEmail")] pub cc_email: Vec<String>,
    #[serde(default, rename = "bccEmail")] pub bcc_email: Vec<String>,
    #[serde(default)] pub text: Option<String>,
    #[serde(default)] pub content: Option<String>,
    #[serde(default)] pub subject: Option<String>,
    #[serde(default)] pub attachments: Vec<AttachmentRequest>,
    #[serde(default, rename = "manyType")] pub many_type: Option<String>,
}

#[derive(Deserialize, Clone)]
pub struct AttachmentRequest {
    #[serde(default)] pub content: String,
    #[serde(default)] pub filename: String,
    #[serde(default)] pub size: Option<i64>,
    #[serde(default, alias = "contentType")] pub mime_type: Option<String>,
    #[serde(default, rename = "contentId")] pub content_id: Option<String>,
}

/// 地址列表归一化：去空白、去空项。
fn normalize_address_list(list: &[String]) -> Vec<String> {
    list.iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
}

pub async fn send(State(state): State<Arc<AppState>>, user: AuthUser, Json(req): Json<SendRequest>) -> Response {
    use crate::handlers::common as cm;

    let mut receive_email = normalize_address_list(&req.receive_email);
    let mut cc_email = normalize_address_list(&req.cc_email);
    let mut bcc_email = normalize_address_list(&req.bcc_email);
    receive_email.retain(|e| e.contains('@'));
    if receive_email.is_empty() {
        return err(400, "emptyRecipient");
    }

    // 抄送/密送去重（原版 normalizeAddressList + takenAddress）
    let mut taken: std::collections::HashSet<String> =
        receive_email.iter().map(|e| e.to_lowercase()).collect();
    cc_email.retain(|e| !taken.contains(&e.to_lowercase()));
    for e in &cc_email { taken.insert(e.to_lowercase()); }
    bcc_email.retain(|e| !taken.contains(&e.to_lowercase()));

    // 分开发送不带抄送和密送
    let separate = req.many_type.as_deref() == Some("divide");
    if separate {
        cc_email.clear();
        bcc_email.clear();
    }
    let copy_count = cc_email.len() + bcc_email.len();
    let quota_count = receive_email.len() + copy_count;

    // 发送总开关（0=OPEN，1=CLOSE）
    let setting = crate::handlers::auth::current_setting(&state).await;
    if setting["send"].as_i64().unwrap_or(0) == 1 {
        return err(403, "disabledSend");
    }

    let domain_allowed = |email: &str| state.config.domain_allowed(email);
    let all_internal = receive_email.iter().chain(&cc_email).chain(&bcc_email)
        .all(|e| domain_allowed(e));

    // 发送账号归属校验
    let account: Option<(i64, String, i64)> = sqlx::query_as(
        "SELECT account_id,email,user_id FROM account WHERE account_id=? AND is_del=0")
        .bind(req.account_id).fetch_optional(&state.pool).await.unwrap_or(None);
    let (account_id, send_email, account_user) = match account {
        Some(v) => v,
        None => return err(400, "senderAccountNotExist"),
    };
    if account_user != user.user_id {
        return err(400, "sendEmailNotCurUser");
    }

    // 发送者与角色
    let user_row: Option<(String, i64, i64)> = sqlx::query_as(
        "SELECT email,type,send_count FROM user WHERE user_id=?")
        .bind(user.user_id).fetch_optional(&state.pool).await.unwrap_or(None);
    let (user_email, _, user_send_count) = match user_row {
        Some(v) => v,
        None => return err(401, "authExpired"),
    };
    let role: Option<(String, Option<i64>, String)> = sqlx::query_as(
        "SELECT send_type,send_count,avail_domain FROM role WHERE role_id=(SELECT type FROM user WHERE user_id=?)")
        .bind(user.user_id).fetch_optional(&state.pool).await.unwrap_or(None);
    let (role_send_type, role_send_count, role_avail) = role.unwrap_or_else(|| ("count".into(), None, String::new()));
    let is_admin = user_email.eq_ignore_ascii_case(&state.config.admin);

    if !is_admin {
        // 发件类型：ban 禁发；internal 仅站内
        if role_send_type == "ban" {
            return err(403, "bannedSend");
        }
        if role_send_type == "internal" && !all_internal {
            return err(403, "onlyInternalSend");
        }
        // 发件额度：day 按天计数（user.send_count 由定时任务重置），count 按总量
        if let Some(limit) = role_send_count.filter(|v| *v > 0) {
            if user_send_count >= limit {
                return err(403, if role_send_type == "day" { "daySendLimit" } else { "totalSendLimit" });
            }
            if user_send_count + quota_count as i64 > limit {
                return err(403, if role_send_type == "day" { "daySendLack" } else { "totalSendLack" });
            }
        }
        // 发件域名权限
        if !cm::has_avail_domain_perm(&role_avail, &send_email) {
            return err(403, "noDomainPermSend");
        }
    }

    // 站外收件人必须有发信通道（原版 noSendProvider）
    let smtp = state.config.smtp.clone();
    if !all_internal && smtp.is_none() {
        return err(400, "noSendProvider");
    }

    let name = req.name.clone().filter(|v| !v.is_empty())
        .unwrap_or_else(|| cm::get_name(&send_email));

    // 回复邮件：校验原邮件并取 messageId（inReplyTo / relation）
    let mut in_reply_to = String::new();
    if req.send_type.as_deref() == Some("reply") {
        let msg_id: Option<String> = sqlx::query_scalar(
            "SELECT message_id FROM email WHERE email_id=? AND user_id=?")
            .bind(req.email_id).bind(user.user_id)
            .fetch_optional(&state.pool).await.unwrap_or(None);
        match msg_id {
            Some(id) => in_reply_to = id,
            None => return err(400, "notExistEmailReply"),
        }
    }

    // 附件数量限制
    let image_count = req.attachments.iter().filter(|a| a.content_id.is_some()).count();
    if image_count > 10 {
        return err(400, "imageAttLimit");
    }
    if req.attachments.len() > 10 {
        return err(400, "attLimit");
    }
    if !all_internal {
        let per_message = (if separate { 1 } else { receive_email.len() }) + copy_count;
        if per_message > 50 {
            return err(400, "recipientLimit");
        }
    }

    let subject = req.subject.clone().unwrap_or_default();
    let text = req.text.clone().unwrap_or_default();
    let content = req.content.clone().unwrap_or_default();
    let recipient_json = |list: &[String]| json!(list.iter()
        .map(|a| json!({"address": a, "name": ""})).collect::<Vec<_>>()).to_string();
    let cc_json = recipient_json(&cc_email);
    let bcc_json = recipient_json(&bcc_email);
    let cc_json_empty = "[]".to_string();

    // 额度扣减与每日统计（原版 saveSentEmail：internal 类型不计数）
    let delivered_count = (receive_email.len() + copy_count) as i64;
    let incr_quota = role_send_count.map(|v| v > 0).unwrap_or(false) && role_send_type != "internal";
    if incr_quota {
        let _ = sqlx::query("UPDATE user SET send_count=send_count+? WHERE user_id=?")
            .bind(delivered_count).bind(user.user_id).execute(&state.pool).await;
    }
    // 原版在 Workers 中用 dayjs()（UTC）作 KV 日期键，普通服务器统一用 UTC
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let kv_key = format!("send_day_count:{}", today);
    let day_total: i64 = crate::kv::get(&state.pool, &kv_key).await
        .and_then(|s| s.parse().ok()).unwrap_or(0) + delivered_count;
    crate::kv::put(&state.pool, &kv_key, &day_total.to_string(), 24 * 3600).await;

    let mut email_results: Vec<Value> = Vec::new();
    let mut failed_recipients: Vec<String> = Vec::new();

    // 收件人分组：普通发送一组全发，分开发送每人一组
    let groups: Vec<Vec<String>> = if separate {
        receive_email.iter().map(|r| vec![r.clone()]).collect()
    } else {
        vec![receive_email.clone()]
    };

    for group in &groups {
        let group_recipient_json = recipient_json(group);
        // 落库发件记录（原版 status：有发信通道 DELIVERED，否则 SENT）
        let status = if smtp.is_some() { cm::STATUS_DELIVERED } else { cm::STATUS_SENT };
        let result = sqlx::query(
            "INSERT INTO email(send_email,name,sender_email,account_id,user_id,subject,code,text,content,cc,bcc,recipient,to_email,to_name,in_reply_to,relation,type,status,unread,create_time) \
             VALUES(?,?,?,?,?,?,'',?,?,?,?,?,?,?,?,?,?,?,0,datetime('now'))",
        )
        .bind(&send_email).bind(&name).bind(&send_email).bind(account_id).bind(user.user_id)
        .bind(&subject).bind(&text).bind(&content)
        .bind(&cc_json).bind(&bcc_json).bind(&group_recipient_json)
        .bind(group.first().cloned().unwrap_or_default())
        .bind(group.first().map(|a| cm::get_name(a)).unwrap_or_default())
        .bind(&in_reply_to).bind(&in_reply_to)
        .bind(cm::EMAIL_TYPE_SEND)
        .bind(status)
        .execute(&state.pool).await;

        let email_id = match result {
            Ok(r) => r.last_insert_rowid(),
            Err(e) => {
                tracing::error!("save sent email failed: {e}");
                return err(500, "sendEmailFailed");
            }
        };

        // 附件落盘并入库
        for att in &req.attachments {
            let bytes = base64::engine::general_purpose::STANDARD.decode(att.content.trim()).unwrap_or_default();
            let key = format!("attachments/{}.{}", hash(&bytes), extension(&att.filename));
            let _ = crate::storage::put_object(&state.config.storage_dir, &key, &bytes).await;
            let _ = sqlx::query(
                "INSERT INTO attachments(user_id,email_id,account_id,key,filename,mime_type,size,type,content_id,related) VALUES(?,?,?,?,?,?,?,0,?,1)")
                .bind(user.user_id).bind(email_id).bind(account_id).bind(&key)
                .bind(&att.filename).bind(&att.mime_type).bind(att.size.unwrap_or(bytes.len() as i64))
                .bind(att.content_id.clone().unwrap_or_default())
                .execute(&state.pool).await;
        }

        // 站外投递：SMTP 失败时中止并向客户端报错（原版语义）
        if let Some(ref smtp_cfg) = smtp {
            if !all_internal {
                let from = format!("{} <{}>", name, send_email);
                if let Err(e) = send_via_smtp_multi(smtp_cfg, &from, group, &cc_email, &subject, &text, &content).await {
                    tracing::warn!("SMTP delivery failed for email {} to {:?}: {}", email_id, group, e);
                    if separate {
                        failed_recipients.extend(group.iter().cloned());
                        continue;
                    }
                    return err(500, &e);
                }
            }
        }

        email_results.push(json!({
            "emailId": email_id, "subject": subject,
            "recipient": group_recipient_json,
            "cc": cc_json, "bcc": bcc_json,
        }));
    }

    // 站内投递：收件人 + 抄送 + 密送各投一份到收件箱
    // （原版 HandleOnSiteEmail：无账号落 NOONE、角色权限不足落 BOUNCED）
    for inbox_addr in receive_email.iter().chain(&cc_email).chain(&bcc_email) {
        let (account_id_in, user_id_in) = match sqlx::query_as::<_, (i64, i64)>(
            "SELECT account_id,user_id FROM account WHERE email=? COLLATE NOCASE AND is_del=0")
            .bind(inbox_addr).fetch_optional(&state.pool).await.unwrap_or(None)
        {
            Some(v) => v,
            None => {
                // `+` 别名回退到主地址
                let base = cm::get_base_email(inbox_addr);
                sqlx::query_as::<_, (i64, i64)>(
                    "SELECT account_id,user_id FROM account WHERE email=? COLLATE NOCASE AND is_del=0")
                    .bind(&base).fetch_optional(&state.pool).await.unwrap_or(None)
                    .unwrap_or_default()
            }
        };

        let (status_in, message) = if account_id_in == 0 {
            (cm::STATUS_NOONE, String::new())
        } else {
            // 收件人角色权限校验（管理员收件不受限）
            let recv_role: Option<(String, String)> = sqlx::query_as(
                "SELECT r.ban_email, r.avail_domain FROM user u JOIN role r ON r.role_id=u.type WHERE u.user_id=?")
                .bind(user_id_in).fetch_optional(&state.pool).await.unwrap_or(None);
            let (ban_email, avail) = recv_role.unwrap_or_default();
            let recv_email: Option<String> = sqlx::query_scalar("SELECT email FROM user WHERE user_id=?")
                .bind(user_id_in).fetch_optional(&state.pool).await.unwrap_or(None);
            let recv_is_admin = recv_email.map(|e| e.eq_ignore_ascii_case(&state.config.admin)).unwrap_or(false);
            if !recv_is_admin && !cm::has_avail_domain_perm(&avail, inbox_addr) {
                (cm::STATUS_BOUNCED, format!("The recipient <{inbox_addr}> is not authorized to use this domain."))
            } else if !recv_is_admin && cm::is_ban_email(&ban_email, &send_email) {
                (cm::STATUS_BOUNCED, format!("The recipient <{inbox_addr}> is disabled from receiving emails."))
            } else {
                (cm::STATUS_RECEIVE, String::new())
            }
        };

        let _ = sqlx::query(
            "INSERT INTO email(send_email,name,sender_email,account_id,user_id,subject,code,text,content,cc,bcc,recipient,to_email,to_name,in_reply_to,relation,message_id,type,status,message,unread,create_time) \
             VALUES(?,?,?,?,?,?,'',?,?,?,'[]',?,?,?,'','','',0,?,?,0,datetime('now'))",
        )
        .bind(&send_email).bind(&name).bind(&send_email)
        .bind(account_id_in).bind(user_id_in)
        .bind(&subject).bind(&text).bind(&content)
        .bind(&cc_json_empty)
        .bind(recipient_json(std::slice::from_ref(inbox_addr)))
        .bind(inbox_addr).bind(cm::get_name(inbox_addr))
        .bind(status_in)
        .bind(&message)
        .execute(&state.pool).await;
    }

    if separate {
        return ok_value(json!({
            "emails": email_results,
            "failedRecipients": failed_recipients,
            "error": if failed_recipients.is_empty() { Value::Null } else { json!("sendEmailFailed") }
        }));
    }
    ok_value(Value::Array(email_results))
}

/// 按 SMTP 配置构建传输层。
/// - tls=false → 明文（builder_dangerous）
/// - tls=true + port 465 → implicit TLS / SMTPS（relay，即 Tls::Wrapper）
/// - tls=true + 其他端口（587/25 等）→ STARTTLS（starttls_relay，即 Tls::Required）
///
/// 注意：lettre 的 `relay()` 是 implicit TLS，不是 STARTTLS。此前 587 误用 relay()
/// 导致对端先发明文 greeting、客户端按 TLS 解析而报 InvalidContentType。
fn build_smtp_transport(
    smtp: &crate::config::SmtpConfig,
) -> Result<lettre::AsyncSmtpTransport<lettre::Tokio1Executor>, String> {
    use lettre::transport::smtp::authentication::Credentials;
    use lettre::{AsyncSmtpTransport, Tokio1Executor};

    let builder = if !smtp.tls {
        AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&smtp.host).port(smtp.port)
    } else if smtp.port == 465 {
        // implicit TLS：连接建立即 TLS 握手，无明文 EHLO
        AsyncSmtpTransport::<Tokio1Executor>::relay(&smtp.host)
            .map_err(|e| e.to_string())?
            .port(smtp.port)
    } else {
        // STARTTLS：先明文连接 + EHLO，再升级 TLS
        AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&smtp.host)
            .map_err(|e| e.to_string())?
            .port(smtp.port)
    };

    Ok(if smtp.user.is_empty() {
        builder.build()
    } else {
        builder
            .credentials(Credentials::new(smtp.user.clone(), smtp.password.clone()))
            .build()
    })
}

/// 用 SMTP relay 原样转发一封已解析的原始邮件（对应原版 CF `message.forward`）。
pub async fn smtp_send_raw(smtp: &crate::config::SmtpConfig, raw: &[u8]) -> Result<(), String> {
    use lettre::address::Address;
    use lettre::address::Envelope;
    use lettre::AsyncTransport;

    // 从原始邮件解析 envelope（发件人与收件人）
    let parsed = mail_parser::MessageParser::default()
        .parse(raw)
        .ok_or_else(|| "failed to parse message".to_string())?;
    let from = match parsed.from() {
        Some(mail_parser::Address::List(list)) => {
            list.first().and_then(|a| a.address.clone())
        }
        Some(mail_parser::Address::Group(groups)) => groups
            .first()
            .and_then(|g| g.addresses.first())
            .and_then(|a| a.address.clone()),
        _ => None,
    };
    let tos = match parsed.to() {
        Some(mail_parser::Address::List(list)) => list
            .iter()
            .filter_map(|a| a.address.clone())
            .collect::<Vec<_>>(),
        Some(mail_parser::Address::Group(groups)) => groups
            .iter()
            .flat_map(|g| g.addresses.iter())
            .filter_map(|a| a.address.clone())
            .collect(),
        None => Vec::new(),
    };
    let sender = from.map(|f| f.parse::<Address>()).transpose().map_err(|e| e.to_string())?;
    let recipients = tos
        .iter()
        .map(|t| t.parse::<Address>())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    if recipients.is_empty() {
        return Err("emptyRecipient".to_string());
    }
    let envelope = Envelope::new(sender, recipients).map_err(|e: lettre::error::Error| e.to_string())?;

    let mailer = build_smtp_transport(smtp)?;
    mailer.send_raw(&envelope, raw).await.map_err(|e| e.to_string())?;
    Ok(())
}

async fn send_via_smtp_multi(
    smtp: &crate::config::SmtpConfig,
    from: &str,
    tos: &[String],
    ccs: &[String],
    subject: &str,
    text: &str,
    html: &str,
) -> Result<(), String> {
    use lettre::message::{MultiPart, SinglePart};
    use lettre::{AsyncTransport, Message};

    let mailer = build_smtp_transport(smtp)?;

    let body = MultiPart::alternative()
        .singlepart(SinglePart::plain(text.to_string()))
        .singlepart(SinglePart::html(html.to_string()));

    let mut builder = Message::builder()
        .from(from.parse().map_err(|e: lettre::address::AddressError| e.to_string())?);
    let mut any_to = false;
    for to in tos {
        if let Ok(addr) = to.parse::<lettre::message::Mailbox>() {
            builder = builder.to(addr);
            any_to = true;
        }
    }
    for cc in ccs {
        if let Ok(addr) = cc.parse::<lettre::message::Mailbox>() {
            builder = builder.cc(addr);
        }
    }
    if !any_to {
        return Err("emptyRecipient".to_string());
    }
    let msg = builder
        .subject(subject.to_string())
        .multipart(body)
        .map_err(|e| e.to_string())?;

    mailer.send(msg).await.map_err(|e| {
        // 透传底层错误文本，原版 sendError 会直接抛给前端
        e.to_string()
    })?;
    Ok(())
}

/// /allEmail/list 的查询参数（原版 allEmailListFilters）。
#[derive(Deserialize)]
pub struct AllListQuery {
    #[serde(default, alias = "emailId")] pub email_id: Option<i64>,
    #[serde(default)] pub size: Option<i64>,
    #[serde(default)] pub name: Option<String>,
    #[serde(default)] pub subject: Option<String>,
    #[serde(default, alias = "accountEmail")] pub account_email: Option<String>,
    #[serde(default, alias = "userEmail")] pub user_email: Option<String>,
    #[serde(default)] pub r#type: Option<String>,
    #[serde(default, alias = "timeSort")] pub time_sort: Option<i64>,
    #[serde(default)] pub full: Option<i64>,
    #[serde(default)] pub text: Option<String>,
    #[serde(default, alias = "searchType")] pub search_type: Option<String>,
    #[serde(default, alias = "startDate")] pub start_date: Option<String>,
    #[serde(default, alias = "endDate")] pub end_date: Option<String>,
}

struct AllFilters { where_sql: String, need_user_join: bool, binds: Vec<FilterBind> }

/// 构造全部邮件的过滤条件（对齐原版 allEmailListFilters）：
/// status != SAVING；type 为 receive/send/delete/noone；各前缀过滤；
/// text+searchType 五种搜索；日期范围；游标。
fn build_all_filters(q: &AllListQuery, with_cursor: bool) -> AllFilters {
    use crate::handlers::common as cm;
    let mut parts: Vec<String> = vec!["e.status<>?".into()];
    let mut binds = vec![FilterBind::Num(cm::STATUS_SAVING)];
    let mut need_user_join = false;

    match q.r#type.as_deref().unwrap_or("receive") {
        "send" => { parts.push("e.type=?".into()); binds.push(FilterBind::Num(cm::EMAIL_TYPE_SEND)); }
        "receive" => { parts.push("e.type=?".into()); binds.push(FilterBind::Num(cm::EMAIL_TYPE_RECEIVE)); }
        "delete" => parts.push("e.is_del=1".into()),
        "noone" => { parts.push("e.status=?".into()); binds.push(FilterBind::Num(cm::STATUS_NOONE)); }
        _ => {}
    }

    let time_sort = q.time_sort.unwrap_or(0);
    if with_cursor {
        if let Some(cursor) = q.email_id.filter(|v| *v > 0) {
            parts.push(if time_sort == 1 { "e.email_id>?" } else { "e.email_id<?" }.into());
            binds.push(FilterBind::Num(cursor));
        }
    }

    // 前缀匹配（原版 LIKE 'xxx%'）
    if let Some(v) = q.user_email.as_deref().filter(|s| !s.trim().is_empty()) {
        need_user_join = true;
        parts.push("u.email COLLATE NOCASE LIKE ?".into());
        binds.push(FilterBind::Text(format!("{}%", v.trim())));
    }
    if let Some(v) = q.account_email.as_deref().filter(|s| !s.trim().is_empty()) {
        // 收件地址或发件地址前缀命中（原版 emailAddressCondition）
        parts.push(
            "e.email_id IN (SELECT email_id FROM email WHERE to_email COLLATE NOCASE LIKE ? \
             UNION SELECT email_id FROM email WHERE send_email COLLATE NOCASE LIKE ?)".into(),
        );
        let pat = format!("{}%", v.trim());
        binds.push(FilterBind::Text(pat.clone()));
        binds.push(FilterBind::Text(pat));
    }
    if let Some(v) = q.name.as_deref().filter(|s| !s.trim().is_empty()) {
        parts.push("e.name COLLATE NOCASE LIKE ?".into());
        binds.push(FilterBind::Text(format!("{}%", v.trim())));
    }
    if let Some(v) = q.subject.as_deref().filter(|s| !s.trim().is_empty()) {
        parts.push("e.subject COLLATE NOCASE LIKE ?".into());
        binds.push(FilterBind::Text(format!("{}%", v.trim())));
    }

    // text + searchType 全文搜索
    if let Some(text) = q.text.as_ref().map(|t| t.trim()).filter(|t| !t.is_empty()) {
        let pattern = format!("%{text}%");
        match q.search_type.as_deref().unwrap_or("") {
            "subject" => { parts.push("e.subject COLLATE NOCASE LIKE ?".into()); binds.push(FilterBind::Text(pattern)); }
            "sender" => { parts.push("e.name COLLATE NOCASE LIKE ?".into()); binds.push(FilterBind::Text(pattern)); }
            "senderAddress" => { parts.push("e.send_email COLLATE NOCASE LIKE ?".into()); binds.push(FilterBind::Text(pattern)); }
            "user" => { need_user_join = true; parts.push("u.email COLLATE NOCASE LIKE ?".into()); binds.push(FilterBind::Text(pattern)); }
            "account" => {
                parts.push(
                    "e.email_id IN (SELECT email_id FROM email WHERE to_email COLLATE NOCASE LIKE ? \
                     UNION SELECT email_id FROM email WHERE send_email COLLATE NOCASE LIKE ?)".into(),
                );
                binds.push(FilterBind::Text(pattern.clone()));
                binds.push(FilterBind::Text(pattern));
            }
            _ => {}
        }
    }

    // 日期范围
    let has_start = q.start_date.as_ref().map(|s| !s.is_empty()).unwrap_or(false);
    let has_end = q.end_date.as_ref().map(|s| !s.is_empty()).unwrap_or(false);
    if has_start || has_end {
        let mut sub: Vec<String> = Vec::new();
        let mut sub_binds: Vec<FilterBind> = Vec::new();
        if has_start { sub.push("e.create_time>=?".into()); sub_binds.push(FilterBind::Text(q.start_date.clone().unwrap())); }
        if has_end { sub.push("e.create_time<=?".into()); sub_binds.push(FilterBind::Text(q.end_date.clone().unwrap())); }
        parts.push(format!(
            "e.email_id IN (SELECT email_id FROM email WHERE {})",
            sub.join(" AND ")
        ));
        binds.extend(sub_binds);
    }

    AllFilters { where_sql: parts.join(" AND "), need_user_join, binds }
}

/// 管理员邮件列表（原版 emailService.allList）。
pub async fn all_list(State(state): State<Arc<AppState>>, _user: AuthUser, Query(q): Query<AllListQuery>) -> Response {
    use crate::handlers::common as cm;
    use crate::handlers::common::row_json_camel;
    let size = q.size.unwrap_or(10).clamp(1, 50);
    let time_sort = q.time_sort.unwrap_or(0);
    let full = q.full.unwrap_or(1) == 1;
    let order = if time_sort == 1 { "ASC" } else { "DESC" };

    let filters = build_all_filters(&q, true);
    // 原版 allList 列表恒 join user 返回 userEmail；count 仅在按用户搜索时 join
    let list_sql = format!(
        "SELECT e.*, u.email AS user_email FROM email AS e \
         LEFT JOIN user AS u ON u.user_id=e.user_id WHERE {where_sql} ORDER BY e.email_id {order} LIMIT ?",
        where_sql = filters.where_sql, order = order,
    );
    let mut list_query = sqlx::query(&list_sql);
    for b in &filters.binds {
        list_query = match b {
            FilterBind::Text(v) => list_query.bind(v.clone()),
            FilterBind::Num(n) => list_query.bind(*n),
        };
    }
    let rows = list_query.bind(size).fetch_all(&state.pool).await.unwrap_or_default();

    let count_filters = build_all_filters(&q, false);
    let count_user_join = if count_filters.need_user_join { "LEFT JOIN user AS u ON u.user_id=e.user_id" } else { "" };
    let count_sql = format!("SELECT COUNT(*) FROM email AS e {count_user_join} WHERE {}", count_filters.where_sql);
    let mut count_query = sqlx::query_scalar::<_, i64>(&count_sql);
    for b in &count_filters.binds {
        count_query = match b {
            FilterBind::Text(v) => count_query.bind(v.clone()),
            FilterBind::Num(n) => count_query.bind(*n),
        };
    }
    let total = count_query.fetch_one(&state.pool).await.unwrap_or(0);

    // 最新收件邮件（同步锚点）
    let latest_email: Value = sqlx::query_as::<_, (i64, i64, i64)>(
        "SELECT email_id,account_id,user_id FROM email WHERE type=? ORDER BY email_id DESC LIMIT 1",
    )
    .bind(cm::EMAIL_TYPE_RECEIVE)
    .fetch_optional(&state.pool)
    .await
    .ok()
    .flatten()
    .map(|r| json!({"emailId": r.0, "accountId": r.1, "userId": r.2}))
    .unwrap_or(json!({"emailId": 0, "accountId": 0, "userId": 0}));

    let mut list: Vec<Value> = Vec::new();
    for row in &rows {
        let mut obj = row_json_camel(row).as_object().cloned().unwrap_or_default();
        let email_id: i64 = row.get("email_id");
        if full {
            let atts = sqlx::query(
                "SELECT att_id,user_id,email_id,account_id,key,filename,mime_type,size,type \
                 FROM attachments WHERE email_id=? AND type=0 ORDER BY att_id",
            )
            .bind(email_id)
            .fetch_all(&state.pool)
            .await
            .unwrap_or_default();
            obj.insert("attList".into(), Value::Array(atts.into_iter().map(att_json).collect()));
        } else {
            let raw = obj.get("text").and_then(|v| v.as_str()).unwrap_or("").to_string();
            obj.insert("listText".into(), json!(cm::to_list_text(&raw)));
            // 摘要模式：有 text 则不返回 content，text 截断到 300 字符
            let text_empty = raw.trim().is_empty();
            let content_val = obj.get("content").cloned().unwrap_or(Value::Null);
            obj.insert("content".into(), if text_empty { content_val } else { Value::Null });
            obj.entry("text".to_string()).and_modify(|v| {
                if let Some(s) = v.as_str() {
                    *v = json!(s.chars().take(300).collect::<String>());
                }
            });
        }
        list.push(Value::Object(obj));
    }
    if full {
        cm::rewrite_list_content(&state, &mut list).await;
    }
    ok_value(json!({"list": list, "total": total, "latestEmail": latest_email}))
}

pub async fn all_delete(State(state): State<Arc<AppState>>, _user: AuthUser, Query(q): Query<IdsQuery>) -> Response {
    // Original /allEmail/delete is a physical delete by the admin.
    let ids = ids_from(q.email_ids.unwrap_or_default());
    crate::handlers::tasks::delete_emails_by_ids(&state.pool, &ids).await;
    ok_value(json!(null))
}

/// 管理员按条件批量物理删除（原版 emailService.batchDelete）。
#[derive(Deserialize)]
pub struct BatchDeleteQuery {
    #[serde(default, alias = "sendName")] pub send_name: Option<String>,
    #[serde(default, alias = "sendEmail")] pub send_email: Option<String>,
    #[serde(default, alias = "toEmail")] pub to_email: Option<String>,
    #[serde(default)] pub subject: Option<String>,
    #[serde(default, alias = "startTime")] pub start_time: Option<String>,
    #[serde(default, alias = "endTime")] pub end_time: Option<String>,
    #[serde(default)] pub r#type: Option<String>,
}

pub async fn all_batch_delete(State(state): State<Arc<AppState>>, _user: AuthUser, Query(q): Query<BatchDeleteQuery>) -> Response {
    let typ = q.r#type.as_deref().unwrap_or("");
    let right = typ == "left" || typ == "include";
    let left = typ == "include";

    let mut parts: Vec<String> = Vec::new();
    let mut binds: Vec<FilterBind> = Vec::new();
    let like = |field: &str, value: &str| -> (String, FilterBind) {
        let pat = format!("{}{}{}", if left { "%" } else { "" }, value, if right { "%" } else { "" });
        (format!("{field} LIKE ?"), FilterBind::Text(pat))
    };
    if let Some(v) = q.send_name.as_deref().filter(|s| !s.is_empty()) {
        let (cond, bind) = like("name", v);
        parts.push(cond); binds.push(bind);
    }
    if let Some(v) = q.subject.as_deref().filter(|s| !s.is_empty()) {
        let (cond, bind) = like("subject", v);
        parts.push(cond); binds.push(bind);
    }
    if let Some(v) = q.send_email.as_deref().filter(|s| !s.is_empty()) {
        let (cond, bind) = like("send_email", v);
        parts.push(cond); binds.push(bind);
    }
    if let Some(v) = q.to_email.as_deref().filter(|s| !s.is_empty()) {
        let (cond, bind) = like("to_email", v);
        parts.push(cond); binds.push(bind);
    }
    if q.start_time.as_deref().map(|s| !s.is_empty()).unwrap_or(false)
        && q.end_time.as_deref().map(|s| !s.is_empty()).unwrap_or(false)
    {
        parts.push("create_time>=?".into());
        binds.push(FilterBind::Text(q.start_time.clone().unwrap()));
        parts.push("create_time<=?".into());
        binds.push(FilterBind::Text(q.end_time.clone().unwrap()));
    }

    // 无条件不删除（原版直接 return）
    if parts.is_empty() {
        return ok_value(json!(null));
    }
    let where_sql = parts.join(" AND ");
    let ids: Vec<i64> = {
        let sql = format!("SELECT email_id FROM email WHERE {where_sql}");
        let mut query = sqlx::query_scalar::<_, i64>(&sql);
        for b in &binds {
            query = match b {
                FilterBind::Text(v) => query.bind(v.clone()),
                FilterBind::Num(n) => query.bind(*n),
            };
        }
        query.fetch_all(&state.pool).await.unwrap_or_default()
    };
    if ids.is_empty() {
        return ok_value(json!(null));
    }
    crate::handlers::tasks::delete_emails_by_ids(&state.pool, &ids).await;
    ok_value(json!(null))
}

/// 管理员新邮件轮询（原版 allEmailLatest）。
pub async fn all_latest(State(state): State<Arc<AppState>>, _user: AuthUser, Query(q): Query<AllListQuery>) -> Response {
    use crate::handlers::common as cm;
    let email_id = q.email_id.unwrap_or(0);
    let rows = sqlx::query(
        "SELECT e.*, u.email AS user_email FROM email AS e \
         LEFT JOIN user AS u ON u.user_id=e.user_id \
         WHERE e.email_id>? AND e.type=? AND e.status<>? ORDER BY e.email_id DESC LIMIT 50",
    )
    .bind(email_id)
    .bind(cm::EMAIL_TYPE_RECEIVE)
    .bind(cm::STATUS_SAVING)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let mut list: Vec<Value> = Vec::new();
    for row in &rows {
        let mut obj = crate::handlers::common::row_json_camel(row).as_object().cloned().unwrap_or_default();
        let id: i64 = row.get("email_id");
        let atts = sqlx::query(
            "SELECT att_id,user_id,email_id,account_id,key,filename,mime_type,size,type \
             FROM attachments WHERE email_id=? AND type=0 ORDER BY att_id",
        )
        .bind(id)
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default();
        obj.insert("attList".into(), Value::Array(atts.into_iter().map(att_json).collect()));
        let raw = obj.get("text").and_then(|v| v.as_str()).unwrap_or("").to_string();
        obj.insert("listText".into(), json!(cm::to_list_text(&raw)));
        list.push(Value::Object(obj));
    }
    cm::rewrite_list_content(&state, &mut list).await;
    ok_value(Value::Array(list))
}

pub async fn analysis(State(state): State<Arc<AppState>>, _user: AuthUser, Query(q): Query<serde_json::Value>) -> Response {
    use sqlx::{Column, Row};
    let time_zone = q.get("timeZone").and_then(|v| v.as_str()).unwrap_or("UTC");

    // 原版 dayjs.tz：按 IANA 时区求「UTC 当日 0 点」的本地偏移（支持半小时区与 DST）
    let diff_hours = tz_offset_hours(time_zone);
    let fmt_hours = |v: f64| {
        if (v - v.round()).abs() < 1e-9 {
            format!("{}", v.round() as i64)
        } else {
            format!("{v}")
        }
    };
    let tz_mod = if diff_hours >= 0.0 {
        format!("+{} hours", fmt_hours(diff_hours))
    } else {
        format!("{} hours", fmt_hours(diff_hours))
    };
    let tz_back = if -diff_hours >= 0.0 {
        format!("+{} hours", fmt_hours(-diff_hours))
    } else {
        format!("{} hours", fmt_hours(-diff_hours))
    };
    // 原版 filterEmptyDay：以目标时区的「今天」为基准向前补 15 天
    let today_local = (chrono::Utc::now()
        + chrono::Duration::seconds((diff_hours * 3600.0).round() as i64))
        .date_naive();

    // Summary counters.
    let number_count: Value = sqlx::query(
        "SELECT \
           COALESCE(e.receiveTotal,0) AS receiveTotal, COALESCE(e.sendTotal,0) AS sendTotal, \
           COALESCE(e.delReceiveTotal,0) AS delReceiveTotal, COALESCE(e.delSendTotal,0) AS delSendTotal, \
           COALESCE(e.normalReceiveTotal,0) AS normalReceiveTotal, COALESCE(e.normalSendTotal,0) AS normalSendTotal, \
           COALESCE(u.userTotal,0) AS userTotal, COALESCE(u.normalUserTotal,0) AS normalUserTotal, COALESCE(u.delUserTotal,0) AS delUserTotal, \
           COALESCE(a.accountTotal,0) AS accountTotal, COALESCE(a.normalAccountTotal,0) AS normalAccountTotal, COALESCE(a.delAccountTotal,0) AS delAccountTotal \
         FROM \
           (SELECT \
              SUM(CASE WHEN type=0 THEN 1 ELSE 0 END) AS receiveTotal, \
              SUM(CASE WHEN type=1 THEN 1 ELSE 0 END) AS sendTotal, \
              SUM(CASE WHEN type=0 AND is_del=1 THEN 1 ELSE 0 END) AS delReceiveTotal, \
              SUM(CASE WHEN type=1 AND is_del=1 THEN 1 ELSE 0 END) AS delSendTotal, \
              SUM(CASE WHEN type=0 AND is_del=0 THEN 1 ELSE 0 END) AS normalReceiveTotal, \
              SUM(CASE WHEN type=1 AND is_del=0 THEN 1 ELSE 0 END) AS normalSendTotal \
            FROM email) e \
         CROSS JOIN \
           (SELECT COUNT(*) AS userTotal, SUM(CASE WHEN is_del=1 THEN 1 ELSE 0 END) AS delUserTotal, \
              SUM(CASE WHEN is_del=0 THEN 1 ELSE 0 END) AS normalUserTotal FROM user) u \
         CROSS JOIN \
           (SELECT COUNT(*) AS accountTotal, SUM(CASE WHEN is_del=1 THEN 1 ELSE 0 END) AS delAccountTotal, \
              SUM(CASE WHEN is_del=0 THEN 1 ELSE 0 END) AS normalAccountTotal FROM account) a",
    )
    .fetch_one(&state.pool)
    .await
    .ok()
    .map(|row| {
        let mut m = serde_json::Map::new();
        for c in row.columns() {
            let n = c.name();
            let v: i64 = row.try_get(n).unwrap_or(0);
            m.insert(n.to_string(), json!(v));
        }
        Value::Object(m)
    })
    .unwrap_or(json!({}));

    // Top senders (name ratio).
    let name_ratio: Value = {
        let rows = sqlx::query(
            "SELECT name, COUNT(*) AS total FROM email \
             WHERE type=0 AND name IS NOT NULL AND name<>'noreply' AND name<>'' \
             GROUP BY name ORDER BY total DESC LIMIT 6",
        )
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default();
        Value::Array(rows.into_iter().map(|r| {
            json!({
                "name": r.get::<Option<String>, _>("name"),
                "total": r.get::<i64, _>("total")
            })
        }).collect())
    };

    // Per-day counts for the past 15 days (excluding today), padded with zeros.
    let user_day = day_counts(&state, "user", &tz_mod, &tz_back, today_local).await;
    let receive_day = day_counts(&state, "email AND type=0", &tz_mod, &tz_back, today_local).await;
    let send_day = day_counts(&state, "email AND type=1", &tz_mod, &tz_back, today_local).await;

    // daySendTotal from KV cache (send_day_count:YYYY-MM-DD, UTC).
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let day_send_total: i64 = crate::kv::get(&state.pool, &format!("send_day_count:{}", today))
        .await
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(0);

    ok_value(json!({
        "numberCount": number_count,
        "userDayCount": user_day,
        "receiveRatio": { "nameRatio": name_ratio },
        "emailDayCount": { "receiveDayCount": receive_day, "sendDayCount": send_day },
        "daySendTotal": day_send_total
    }))
}

/// Count rows per day over the last 15 completed days (excluding today), in
/// the supplied timezone offset, and pad missing days with zero. `source` is
/// either "user", "email AND type=0" or "email AND type=1".
async fn day_counts(
    state: &Arc<AppState>,
    source: &str,
    tz_mod: &str,
    tz_back: &str,
    today: chrono::NaiveDate,
) -> Value {
    use sqlx::Row;
    let table = source.split_whitespace().next().unwrap_or("email");
    let extra = source.strip_prefix(table).unwrap_or("").trim();
    // `extra` may already begin with "AND ..." (e.g. "email AND type=0").
    let where_extra = if extra.is_empty() {
        String::new()
    } else if extra.to_uppercase().starts_with("AND") {
        format!(" {}", extra)
    } else {
        format!(" AND {}", extra)
    };
    let sql = format!(
        "SELECT DATE(create_time, ?) AS date, COUNT(*) AS total FROM {table} \
         WHERE 1=1{where_extra} \
           AND create_time >= datetime('now', ?, 'start of day', '-15 days', ?) \
           AND create_time < datetime('now', ?, 'start of day', ?) \
         GROUP BY DATE(create_time, ?) ORDER BY date ASC"
    );
    let rows = sqlx::query(&sql)
        .bind(tz_mod)
        .bind(tz_mod).bind(tz_back)
        .bind(tz_mod).bind(tz_back)
        .bind(tz_mod)
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default();
    let counts: std::collections::HashMap<String, i64> = rows.iter().map(|r| {
        (r.get::<Option<String>, _>("date").unwrap_or_default(), r.get::<i64, _>("total"))
    }).collect();

    // Build the last 15 completed days in the requested timezone.
    let mut out: Vec<Value> = Vec::new();
    for i in (1..=15).rev() {
        let day = today - chrono::Duration::days(i);
        let key = day.format("%Y-%m-%d").to_string();
        let total = counts.get(&key).copied().unwrap_or(0);
        out.push(json!({"date": key, "total": total}));
    }
    Value::Array(out)
}

/// 原版 dayjs.tz 的 IANA 时区偏移（小时，支持 5.5/5.75 与夏令时）。
/// 无法识别时回退解析 `UTC+8` / `GMT-5` / `+08:00` 形式，最终默认 0。
fn tz_offset_hours(tz: &str) -> f64 {
    if let Ok(tz) = tz.parse::<chrono_tz::Tz>() {
        let now = chrono::Utc::now();
        let utc_start = chrono::TimeZone::with_ymd_and_hms(
            &chrono::Utc, now.year(), now.month(), now.day(), 0, 0, 0,
        )
        .single()
        .unwrap_or(now);
        let local = utc_start.with_timezone(&tz);
        let seconds = local.naive_local() - utc_start.naive_utc();
        return seconds.num_seconds() as f64 / 3600.0;
    }
    let raw = tz.trim();
    for prefix in ["UTC", "GMT"] {
        if let Some(rest) = raw.strip_prefix(prefix) {
            return parse_signed_offset(rest);
        }
    }
    parse_signed_offset(raw)
}

fn parse_signed_offset(rest: &str) -> f64 {
    let (sign, num) = match rest.chars().next() {
        Some('+') => (1.0, &rest[1..]),
        Some('-') => (-1.0, &rest[1..]),
        _ => (1.0, rest),
    };
    if num.is_empty() {
        return 0.0;
    }
    let mut parts = num.split(':');
    let hours = parts.next().and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
    let minutes = parts.next().and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
    sign * (hours + minutes / 60.0)
}

fn ids_from(raw: String) -> Vec<i64> {
    raw.split(',').filter_map(|v| v.trim().parse().ok()).collect()
}
fn hash(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new(); hasher.update(bytes);
    base64::engine::general_purpose::STANDARD.encode(hasher.finalize())
}
fn extension(filename: &str) -> String {
    filename.rsplit('.').next().unwrap_or("bin").chars().take(16).collect()
}
fn err(code: u16, message: &str) -> Response {
    (axum::http::StatusCode::OK, Json(json!({"code":code,"message":message}))).into_response()
}
