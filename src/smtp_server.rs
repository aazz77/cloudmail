use anyhow::Result;
use base64::Engine;
use mail_parser::{MessageParser, MimeHeaders};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

use crate::models::AppState;

/// 邮件处理结果分类：策略性拒收（对齐原版 message.setReject，回复 550）与
/// 临时故障（数据库/存储错误，回复 451）。
enum MailError {
    Reject(String),
    Transient(anyhow::Error),
}

impl From<anyhow::Error> for MailError {
    fn from(e: anyhow::Error) -> Self {
        MailError::Transient(e)
    }
}

impl From<sqlx::Error> for MailError {
    fn from(e: sqlx::Error) -> Self {
        MailError::Transient(e.into())
    }
}

pub async fn run(listener: TcpListener, state: Arc<AppState>) -> Result<()> {
    tracing::info!("SMTP server listening on {}", listener.local_addr()?);
    loop {
        let (socket, addr) = listener.accept().await?;
        let state = state.clone();
        tokio::spawn(async move {
            if let Err(e) = handle(socket, addr, state).await {
                tracing::debug!("SMTP connection from {} ended: {}", addr, e);
            }
        });
    }
}

struct SmtpSession {
    mail_from: Option<String>,
    rcpt_to: Vec<String>,
    data_lines: Vec<String>,
    in_data: bool,
}

impl SmtpSession {
    fn new() -> Self {
        Self {
            mail_from: None,
            rcpt_to: Vec::new(),
            data_lines: Vec::new(),
            in_data: false,
        }
    }

    fn reset(&mut self) {
        self.mail_from = None;
        self.rcpt_to.clear();
        self.data_lines.clear();
        self.in_data = false;
    }
}

async fn handle(socket: TcpStream, _addr: std::net::SocketAddr, state: Arc<AppState>) -> Result<()> {
    let (read_half, mut write_half) = socket.into_split();
    let mut reader = BufReader::new(read_half);
    let mut line = String::new();

    write_half
        .write_all(format!("220 {} ESMTP\r\n", state.config.smtp_hostname).as_bytes())
        .await?;

    let mut session = SmtpSession::new();

    loop {
        line.clear();
        let bytes = reader.read_line(&mut line).await?;
        if bytes == 0 {
            return Ok(());
        }

        let cmd = line.trim_end_matches("\r\n").trim_end_matches('\n');
        let upper = cmd.to_uppercase();

        if session.in_data {
            if cmd == "." {
                session.in_data = false;
                let raw = session.data_lines.join("\n");
                match process_email(&state, &session.mail_from, &session.rcpt_to, &raw).await {
                    Ok(_) => {
                        write_half.write_all(b"250 OK\r\n").await?;
                    }
                    Err(MailError::Reject(msg)) => {
                        tracing::info!("Email rejected: {}", msg);
                        write_half
                            .write_all(format!("550 {}\r\n", msg.replace('\r', " ").replace('\n', " "))
                                .as_bytes())
                            .await?;
                    }
                    Err(MailError::Transient(e)) => {
                        tracing::warn!("Failed to process email: {}", e);
                        write_half.write_all(b"451 Temporary failure\r\n").await?;
                    }
                }
                session.reset();
            } else {
                // RFC 5321 4.5.2: leading dot is escaped as two dots
                let unescaped = if let Some(stripped) = cmd.strip_prefix("..") {
                    stripped
                } else {
                    cmd
                };
                session.data_lines.push(unescaped.to_string());
            }
            continue;
        }

        if upper.starts_with("HELO") || upper.starts_with("EHLO") {
            write_half.write_all(b"250 cloud-mail\r\n").await?;
        } else if upper.starts_with("MAIL FROM:") {
            session.mail_from = Some(extract_address(&cmd[10..]));
            write_half.write_all(b"250 OK\r\n").await?;
        } else if upper.starts_with("RCPT TO:") {
            let address = extract_address(&cmd[8..]);
            if state.config.domain_allowed(&address) {
                session.rcpt_to.push(address);
                write_half.write_all(b"250 OK\r\n").await?;
            } else {
                write_half.write_all(b"550 Relaying denied\r\n").await?;
            }
        } else if upper == "DATA" {
            if session.mail_from.is_none() || session.rcpt_to.is_empty() {
                write_half.write_all(b"503 Bad sequence of commands\r\n").await?;
            } else {
                session.in_data = true;
                write_half
                    .write_all(b"354 End data with <CR><LF>.<CR><LF>\r\n")
                    .await?;
            }
        } else if upper == "QUIT" {
            write_half.write_all(b"221 Bye\r\n").await?;
            return Ok(());
        } else if upper == "RSET" {
            session.reset();
            write_half.write_all(b"250 OK\r\n").await?;
        } else if upper == "NOOP" {
            write_half.write_all(b"250 OK\r\n").await?;
        } else {
            write_half.write_all(b"500 Unknown command\r\n").await?;
        }
    }
}

fn extract_address(raw: &str) -> String {
    let raw = raw.trim();
    if let Some(start) = raw.find('<') {
        if let Some(end) = raw[start..].find('>') {
            return raw[start + 1..start + end].to_string();
        }
    }
    raw.split_whitespace()
        .next()
        .unwrap_or("")
        .trim_start_matches('<')
        .trim_end_matches('>')
        .to_string()
}

async fn process_email(
    state: &Arc<AppState>,
    mail_from: &Option<String>,
    rcpt_to: &[String],
    raw: &str,
) -> Result<(), MailError> {
    use crate::handlers::common as cm;

    let setting = crate::handlers::auth::current_setting(state).await;
    let s_i64 = |key: &str| setting.get(key).and_then(|v| v.as_i64()).unwrap_or(0);
    let s_str = |key: &str| {
        setting.get(key).and_then(|v| v.as_str()).unwrap_or("").to_string()
    };

    // 收件总开关（0=OPEN，1=CLOSE → 整体拒收，原版 setReject('Service suspended')）
    if s_i64("receive") == 1 {
        return Err(MailError::Reject("Service suspended".into()));
    }

    let message = MessageParser::default()
        .parse(raw.as_bytes())
        .ok_or_else(|| MailError::Transient(anyhow::anyhow!("failed to parse MIME message")))?;

    let subject = message.subject().unwrap_or("").to_string();
    let (from_addr, from_name) = extract_header_address(message.from());
    let message_id = message.message_id().unwrap_or("").to_string();
    let text_body = message.body_text(0).unwrap_or_default().to_string();
    let html_body = message.body_html(0).unwrap_or_default().to_string();

    // 原版：sendEmail = 解析后的 From 地址；senderEmail = 原始 From 头（含显示名）。
    // envelope MAIL FROM 仅作 From 头缺失时的兜底。
    let sender = from_addr
        .clone()
        .or_else(|| mail_from.clone())
        .unwrap_or_default();
    let sender_email_raw = message
        .header_raw(mail_parser::HeaderName::From)
        .unwrap_or("")
        .to_string();

    // 黑名单（原版 checkBlock）：主题/内容包含、发件地址或其域名精确命中
    let black_subject = s_str("black_subject");
    let black_content = s_str("black_content");
    let black_from = s_str("black_from");
    let split_non_empty = |v: &str| -> Vec<String> {
        v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
    };
    let blocked = split_non_empty(&black_subject).iter().any(|b| subject.contains(b.as_str()))
        || split_non_empty(&black_content).iter().any(|b| text_body.contains(b.as_str()) || html_body.contains(b.as_str()))
        || split_non_empty(&black_from).iter().any(|b| {
            sender.eq_ignore_ascii_case(b) || cm::get_domain(&sender.to_lowercase()) == b.to_lowercase()
        });
    if blocked {
        return Err(MailError::Reject("Message rejected".into()));
    }

    // 解析 cc / to / in-reply-to / references
    let to_addresses = header_addresses(message.to());
    let cc_addresses = header_addresses(message.cc());
    let bcc_addresses = header_addresses(message.bcc());
    let in_reply_to = message
        .in_reply_to()
        .as_text()
        .unwrap_or("")
        .to_string();
    let references = message
        .references()
        .as_text()
        .unwrap_or("")
        .to_string();
    let recipient_json = |list: &[(Option<String>, Option<String>)]| {
        serde_json::json!(list.iter()
            .map(|(addr, name)| serde_json::json!({"address": addr, "name": name}))
            .collect::<Vec<_>>())
        .to_string()
    };
    let cc_json = recipient_json(&cc_addresses);
    let bcc_json = recipient_json(&bcc_addresses);

    // CID 内嵌图：记录 content_id → key，供 imgReplace 使用
    let _r2_domain = s_str("r2_domain");
    let mut cid_keys: Vec<(String, String)> = Vec::new();

    for recipient in rcpt_to {
        // 账号查找：精确匹配（含已删除账号，原版 selectByEmailIncludeDel），
        // `+` 别名回退主地址
        let account: Option<(i64, i64)> = sqlx::query_as(
            "SELECT account_id, user_id FROM account WHERE email = ? COLLATE NOCASE",
        )
        .bind(recipient)
        .fetch_optional(&state.pool)
        .await
        .map_err(MailError::from)?;
        let account = match account {
            Some(v) => Some(v),
            None if recipient.contains('+') => {
                let base = cm::get_base_email(recipient);
                sqlx::query_as(
                    "SELECT account_id, user_id FROM account WHERE email = ? COLLATE NOCASE",
                )
                .bind(&base)
                .fetch_optional(&state.pool)
                .await
                .map_err(MailError::from)?
            }
            _ => None,
        };

        // 无账号且 noRecipient=CLOSE → 拒收（原版 setReject('Recipient not found')）
        let no_recipient = s_i64("no_recipient");
        if account.is_none() && no_recipient == 1 {
            return Err(MailError::Reject("Recipient not found".into()));
        }

        let (account_id, user_id) = account.unwrap_or((0, 0));

        // 收件人角色权限（原版：非管理员才校验）
        if account.is_some() {
            let user_email: Option<String> = sqlx::query_scalar(
                "SELECT email FROM user WHERE user_id=?")
                .bind(user_id)
                .fetch_optional(&state.pool)
                .await
                .map_err(MailError::from)?;
            let recv_is_admin = user_email
                .map(|e| e.eq_ignore_ascii_case(&state.config.admin))
                .unwrap_or(false);
            if !recv_is_admin {
                let role: Option<(String, String)> = sqlx::query_as(
                    "SELECT r.ban_email, r.avail_domain FROM user u JOIN role r ON r.role_id=u.type WHERE u.user_id=?")
                    .bind(user_id)
                    .fetch_optional(&state.pool)
                    .await
                    .map_err(MailError::from)?;
                if let Some((ban_email, avail_domain)) = role {
                    if !cm::has_avail_domain_perm(&avail_domain, recipient) {
                        return Err(MailError::Reject(
                            "The recipient is not authorized to use this domain.".into(),
                        ));
                    }
                    if cm::is_ban_email(&ban_email, &sender) {
                        return Err(MailError::Reject(
                            "The recipient is disabled from receiving emails.".into(),
                        ));
                    }
                }
            }
        }

        let to_name = to_addresses
            .iter()
            .find(|(addr, _)| addr.as_deref().map(|a| a.eq_ignore_ascii_case(recipient)).unwrap_or(false))
            .and_then(|(_, name)| name.clone())
            .unwrap_or_else(|| cm::get_name(recipient));
        let from_display = from_name.clone().unwrap_or_else(|| {
            if sender.is_empty() { recipient.to_string() } else { cm::get_name(&sender) }
        });

        let result = sqlx::query(
            "INSERT INTO email(send_email,name,sender_email,account_id,user_id,subject,code,text,content,cc,bcc,recipient,to_email,to_name,in_reply_to,relation,message_id,type,status,is_del,unread,create_time) \
             VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,0,?,0,0,datetime('now'))",
        )
        .bind(&sender)
        .bind(&from_display)
        .bind(&sender_email_raw)
        .bind(account_id)
        .bind(user_id)
        .bind(&subject)
        .bind("")
        .bind(&text_body)
        .bind(&html_body)
        .bind(&cc_json)
        .bind(&bcc_json)
        .bind(recipient_json(&to_addresses))
        .bind(recipient)
        .bind(&to_name)
        .bind(&in_reply_to)
        .bind(&references)
        .bind(&message_id)
        // 原版 receive→completeReceive 两步收敛：有账号 RECEIVE(0)，无账号 NOONE(7)
        .bind(if account.is_some() { cm::STATUS_RECEIVE } else { cm::STATUS_NOONE })
        .execute(&state.pool)
        .await
        .map_err(MailError::from)?;

        let email_id = result.last_insert_rowid();

        // 验证码提取（正则降级，对齐原版 AI 提取）
        let code = extract_code(&subject, &text_body);
        if !code.is_empty() {
            let _ = sqlx::query("UPDATE email SET code=? WHERE email_id=?")
                .bind(&code)
                .bind(email_id)
                .execute(&state.pool)
                .await
                .map_err(MailError::from)?;
        }

        // 附件与 CID 内嵌图
        for attachment in message.attachments() {
            let body: Vec<u8> = match &attachment.body {
                mail_parser::PartType::Binary(b) => b.to_vec(),
                mail_parser::PartType::InlineBinary(b) => b.to_vec(),
                mail_parser::PartType::Text(t) => t.as_bytes().to_vec(),
                mail_parser::PartType::Html(h) => h.as_bytes().to_vec(),
                _ => continue,
            };

            let filename = find_header(&attachment.headers, |n| matches!(n, mail_parser::HeaderName::ContentDisposition))
                .and_then(|v| match v {
                    mail_parser::HeaderValue::ContentType(ct) => ct.attributes.as_ref()
                        .and_then(|attrs| attrs.iter().find(|(k, _)| k.eq_ignore_ascii_case("filename")))
                        .map(|(_, v)| v.to_string()),
                    mail_parser::HeaderValue::Text(t) => extract_filename_from_disposition(t),
                    _ => None,
                })
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "attachment".to_string());

            let mime_type = find_header(&attachment.headers, |n| matches!(n, mail_parser::HeaderName::ContentType))
                .and_then(|v| match v {
                    mail_parser::HeaderValue::ContentType(ct) => Some(match ct.c_subtype.as_ref() {
                        Some(sub) => format!("{}/{}", ct.c_type, sub),
                        None => ct.c_type.to_string(),
                    }),
                    _ => None,
                })
                .unwrap_or_else(|| "application/octet-stream".to_string());

            let size = body.len() as i64;
            let content_id = attachment
                .content_id()
                .map(|v| v.trim_start_matches('<').trim_end_matches('>').to_string())
                .unwrap_or_default();

            let key = format!(
                "attachments/{}.{}",
                hash_bytes(&body),
                extension(&filename)
            );
            crate::storage::put_object(&state.config.storage_dir, &key, &body)
                .await
                .map_err(MailError::from)?;

            // 有 content_id 的是内嵌图（type=1 EMBED），其余是普通附件（type=0）
            let is_embed = !content_id.is_empty();
            if is_embed {
                cid_keys.push((content_id.clone(), key.clone()));
            }
            sqlx::query(
                "INSERT INTO attachments(user_id,email_id,account_id,key,filename,mime_type,size,status,type,disposition,related,content_id,encoding) VALUES(?,?,?,?,?,?,?,0,?,?,?,?,?)",
            )
            .bind(user_id)
            .bind(email_id)
            .bind(account_id)
            .bind(&key)
            .bind(&filename)
            .bind(&mime_type)
            .bind(size)
            .bind(if is_embed { 1i64 } else { 0i64 })
            .bind(if is_embed { "inline" } else { "attachment" })
            .bind(if is_embed { "1" } else { "" })
            .bind(&content_id)
            .bind("")
            .execute(&state.pool)
            .await
            .map_err(MailError::from)?;
        }

        // CID 图替换：`cid:xxx` → `{{domain}}{key}`（前端把 {{domain}} 替换为存储域名）
        if !cid_keys.is_empty() && !html_body.is_empty() {
            let mut replaced = html_body.clone();
            for (cid, key) in &cid_keys {
                replaced = replaced.replace(&format!("cid:{}", cid), &format!("{{{{domain}}}}{}", key));
            }
            let _ = sqlx::query("UPDATE email SET content=? WHERE email_id=?")
                .bind(&replaced)
                .bind(email_id)
                .execute(&state.pool)
                .await
                .map_err(MailError::from)?;
        }

        tracing::info!(
            "Received email {} for account {} (user {}) from {}",
            email_id,
            account_id,
            user_id,
            sender
        );

        // 收信后通知链（原版：ruleType 匹配 → Telegram → 转发 → Webhook）
        let user_email_notify = if account.is_some() {
            sqlx::query_scalar::<_, Option<String>>(
                "SELECT email FROM user WHERE user_id=?",)
                .bind(user_id)
                .fetch_optional(&state.pool)
                .await
                .map_err(MailError::from)?
                .flatten()
                .unwrap_or_default()
        } else {
            String::new()
        };

        // 转发规则：RULE 模式下仅当收件用户命中 ruleEmail 名单才继续通知
        let rule_type = s_i64("rule_type");
        let rule_email = s_str("rule_email");
        let rule_matched = rule_type != 1 || {
            let users: Vec<String> = split_non_empty(&rule_email)
                .iter().map(|s| s.to_lowercase()).collect();
            !user_email_notify.is_empty() && users.contains(&user_email_notify.to_lowercase())
        };

        if rule_matched {
            let tg_bot_status = s_i64("tg_bot_status");
            let tg_chat_id = s_str("tg_chat_id");
            if tg_bot_status == 0 && !tg_chat_id.is_empty() {
                crate::handlers::telegram::send_to_bot(
                    state,
                    email_id,
                    &code,
                    &subject,
                    &from_display,
                )
                .await;
            }

            // Webhook 推送（原版 webhookService.sendEmail：Authorization + 重试）
            let webhook_status = s_i64("webhook_status");
            let webhook_url = s_str("webhook_url");
            if webhook_status == 0 && !webhook_url.is_empty() {
                let webhook_retry = s_i64("webhook_retry");
                let webhook_secret = s_str("webhook_secret");
                let create_time: Option<String> = sqlx::query_scalar(
                    "SELECT create_time FROM email WHERE email_id=?")
                    .bind(email_id)
                    .fetch_optional(&state.pool)
                    .await
                    .map_err(MailError::from)?;
                let body = serde_json::json!({
                    "emailId": email_id,
                    "sendEmail": sender,
                    "sendName": from_display,
                    "toEmail": recipient,
                    "toName": to_name,
                    "subject": subject,
                    "text": text_body,
                    "content": html_body,
                    "code": code,
                    "createTime": create_time
                });
                let client = reqwest::Client::new();
                let mut last_err = String::new();
                for _ in 0..=webhook_retry.max(0) {
                    let mut req = client.post(&webhook_url).json(&body);
                    if !webhook_secret.is_empty() {
                        req = req.header("Authorization", &webhook_secret);
                    }
                    match req.send().await {
                        Ok(resp) if resp.status().is_success() => { last_err = String::new(); break; }
                        Ok(resp) => last_err = format!("status: {}", resp.status()),
                        Err(e) => last_err = e.to_string(),
                    }
                }
                if !last_err.is_empty() {
                    tracing::error!("Webhook 推送失败: {last_err}");
                }
            }

            // 转发到其他邮箱：用 SMTP relay 原样转发整封邮件
            let forward_status = s_i64("forward_status");
            let forward_email = s_str("forward_email");
            if forward_status == 0 && !forward_email.is_empty() {
                if let Some(ref smtp_cfg) = state.config.smtp {
                    for target in split_non_empty(&forward_email) {
                        if let Err(e) = crate::handlers::emails::smtp_send_raw(smtp_cfg, raw.as_bytes()).await {
                            tracing::error!("转发邮箱 {target} 失败: {e}");
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

/// 取地址列表（原版 email.to / email.cc 为 [{address, name}] 数组）。
fn header_addresses(addr: Option<&mail_parser::Address>) -> Vec<(Option<String>, Option<String>)> {
    match addr {
        Some(mail_parser::Address::List(list)) => list
            .iter()
            .map(|a| (a.address.as_ref().map(|s| s.to_string()), a.name.as_ref().map(|s| s.to_string())))
            .collect(),
        Some(mail_parser::Address::Group(groups)) => groups
            .iter()
            .flat_map(|g| g.addresses.iter())
            .map(|a| (a.address.as_ref().map(|s| s.to_string()), a.name.as_ref().map(|s| s.to_string())))
            .collect(),
        None => Vec::new(),
    }
}

fn find_header<'a, F>(
    headers: &'a [mail_parser::Header<'a>],
    pred: F,
) -> Option<&'a mail_parser::HeaderValue<'a>>
where
    F: Fn(&mail_parser::HeaderName) -> bool,
{
    headers.iter().find(|h| pred(&h.name)).map(|h| &h.value)
}

fn extract_header_address(addr: Option<&mail_parser::Address>) -> (Option<String>, Option<String>) {
    match addr {
        Some(mail_parser::Address::List(addrs)) => {
            addrs.first().map_or((None, None), |a| {
                (
                    a.address.as_ref().map(|s| s.to_string()),
                    a.name.as_ref().map(|s| s.to_string()),
                )
            })
        }
        Some(mail_parser::Address::Group(groups)) => groups
            .first()
            .and_then(|g| g.addresses.first())
            .map_or((None, None), |a| {
                (
                    a.address.as_ref().map(|s| s.to_string()),
                    a.name.as_ref().map(|s| s.to_string()),
                )
            }),
        None => (None, None),
    }
}

fn extract_filename_from_disposition(value: &str) -> Option<String> {
    for part in value.split(';') {
        let part = part.trim();
        if let Some(rest) = part.strip_prefix("filename=") {
            return Some(rest.trim_matches('"').to_string());
        }
        if let Some(rest) = part.strip_prefix("filename*=UTF-8''") {
            return Some(rest.trim_matches('"').to_string());
        }
    }
    None
}

/// Extract a short verification code from the subject/body using heuristics.
/// The original uses Cloudflare AI for this; the Rust build has no AI binding,
/// so we detect common patterns (e.g. "验证码: 123456", "code: AB12CD").
fn extract_code(subject: &str, body: &str) -> String {
    let combined = format!("{}\n{}", subject, body);
    // Look for a labelled numeric/alphanumeric code (<= 8 chars, no spaces).
    let patterns = [
        r"(?i)(?:验证码|校验码|code|otp|token)\s*[:：为是]?\s*([A-Za-z0-9]{4,8})",
        r"(?i)\b([A-Z0-9]{6,8})\b\s*(?:is your|是您的|验证码)",
    ];
    for pat in patterns {
        if let Ok(re) = regex::Regex::new(pat) {
            if let Some(caps) = re.captures(&combined) {
                if let Some(m) = caps.get(1) {
                    let code = m.as_str().trim();
                    if !code.is_empty() && code.len() <= 8 && !code.contains(char::is_whitespace) {
                        return code.to_string();
                    }
                }
            }
        }
    }
    String::new()
}

fn hash_bytes(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    base64::engine::general_purpose::STANDARD.encode(hasher.finalize())
}

fn extension(filename: &str) -> String {
    filename
        .rsplit('.')
        .next()
        .unwrap_or("bin")
        .chars()
        .take(16)
        .collect()
}

/// Read the raw email from a SMTP socket, used for testing.
#[allow(dead_code)]
pub async fn receive_test_email(
    host: &str,
    port: u16,
    from: &str,
    to: &str,
    subject: &str,
    body: &str,
) -> Result<String> {
    let mut stream = TcpStream::connect(format!("{}:{}", host, port)).await?;
    let (rd, mut wr) = stream.split();
    let mut rd = BufReader::new(rd);
    let mut response = String::new();

    read_response(&mut rd, &mut response).await?;

    send_line(&mut wr, &format!("HELO test-client")).await?;
    read_response(&mut rd, &mut response).await?;

    send_line(&mut wr, &format!("MAIL FROM:<{}>", from)).await?;
    read_response(&mut rd, &mut response).await?;

    send_line(&mut wr, &format!("RCPT TO:<{}>", to)).await?;
    read_response(&mut rd, &mut response).await?;

    send_line(&mut wr, "DATA").await?;
    read_response(&mut rd, &mut response).await?;

    let email = format!(
        "From: {}\r\nTo: {}\r\nSubject: {}\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n{}\r\n.\r\n",
        from, to, subject, body
    );
    wr.write_all(email.as_bytes()).await?;
    read_response(&mut rd, &mut response).await?;

    send_line(&mut wr, "QUIT").await?;

    Ok(response)
}

async fn send_line<W: AsyncWriteExt + Unpin>(writer: &mut W, line: &str) -> Result<()> {
    writer
        .write_all(format!("{}\r\n", line).as_bytes())
        .await?;
    Ok(())
}

async fn read_response<R: AsyncBufReadExt + Unpin>(
    reader: &mut R,
    buffer: &mut String,
) -> Result<String> {
    let mut line = String::new();
    reader.read_line(&mut line).await?;
    let trimmed = line.trim_end().to_string();
    buffer.push_str(&trimmed);
    buffer.push('\n');
    Ok(trimmed)
}
