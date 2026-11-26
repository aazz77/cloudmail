//! 与原版 `utils/email-utils.js`、`service/role-service.js` 及 Drizzle 实体
//! 对齐的共享语义助手：邮箱地址处理、角色收发权限判断、数据库行 → camelCase JSON。

use serde_json::{json, Map, Value};

/// 邮件 type 常量（原版 emailConst.type）。
pub const EMAIL_TYPE_SEND: i64 = 1;
pub const EMAIL_TYPE_RECEIVE: i64 = 0;

/// 邮件 status 常量（原版 emailConst.status）。
pub const STATUS_RECEIVE: i64 = 0;
pub const STATUS_SENT: i64 = 1;
pub const STATUS_DELIVERED: i64 = 2;
pub const STATUS_BOUNCED: i64 = 3;
pub const STATUS_SAVING: i64 = 6;
pub const STATUS_NOONE: i64 = 7;

/// 取域名（不含 @）；无效地址返回空串。
pub fn get_domain(email: &str) -> String {
    match email.split_once('@') {
        Some((_, domain)) => domain.to_string(),
        None => String::new(),
    }
}

/// 取本地部分作为默认姓名（原版 emailUtils.getName）。
pub fn get_name(email: &str) -> String {
    match email.split_once('@') {
        Some((local, _)) => local.to_string(),
        None => email.to_string(),
    }
}

/// `user+tag@domain` → `user@domain`（原版 emailUtils.getBaseEmail）。
pub fn get_base_email(email: &str) -> String {
    match email.split_once('@') {
        Some((local, domain)) => format!("{}@{}", local.split('+').next().unwrap_or(""), domain),
        None => String::new(),
    }
}

/// 角色可用域名权限（原版 roleService.hasAvailDomainPerm）。
/// 空列表 = 不限制；否则要求收发地址域名精确命中。
pub fn has_avail_domain_perm(avail_domain: &str, email: &str) -> bool {
    let list: Vec<String> = avail_domain
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect();
    if list.is_empty() {
        return true;
    }
    let domain = get_domain(&email.to_lowercase());
    list.iter().any(|item| *item == domain)
}

/// 发件人拉黑判断（原版 roleService.isBanEmail）。
/// `*` 拦截全部；域名条目按收件域名匹配；其余按完整地址匹配。
pub fn is_ban_email(ban_email: &str, from_email: &str) -> bool {
    let list: Vec<String> = ban_email
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if list.iter().any(|item| item == "*") {
        return true;
    }
    let from_lower = from_email.to_lowercase();
    let from_domain = get_domain(&from_lower);
    for item in list {
        let item_lower = item.to_lowercase();
        // 域名条目（含点且不含 @）
        if item_lower.contains('.') && !item_lower.contains('@') {
            if item_lower == from_domain {
                return true;
            }
        } else if item_lower == from_lower {
            return true;
        }
    }
    false
}

/// snake_case → camelCase（`email_id` → `emailId`，与 Drizzle 实体命名一致）。
pub fn snake_to_camel(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut upper_next = false;
    for ch in name.chars() {
        if ch == '_' {
            upper_next = true;
        } else if upper_next {
            out.extend(ch.to_uppercase());
            upper_next = false;
        } else {
            out.push(ch);
        }
    }
    out
}

/// 把一行查询结果序列化为 camelCase JSON 对象。
/// 值类型依次尝试 TEXT / INTEGER / REAL，与 SQLite 动态类型对齐。
pub fn row_json_camel(row: &sqlx::sqlite::SqliteRow) -> Value {
    use sqlx::{Column, Row};
    let mut map = Map::new();
    for col in row.columns() {
        let name = col.name();
        let value = row
            .try_get::<Option<String>, _>(name)
            .ok()
            .flatten()
            .map(Value::String)
            .or_else(|| {
                row.try_get::<Option<i64>, _>(name)
                    .ok()
                    .flatten()
                    .map(|v| json!(v))
            })
            .or_else(|| {
                row.try_get::<Option<f64>, _>(name)
                    .ok()
                    .flatten()
                    .map(|v| json!(v))
            })
            .unwrap_or(Value::Null);
        map.insert(snake_to_camel(name), value);
    }
    Value::Object(map)
}

/// 列表摘要文本（原版 toListText / emailBriefColumns 的 listText）：
/// 压缩空白后截取 300 字符。
pub fn to_list_text(raw: &str) -> String {
    let collapsed: String = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed.chars().take(300).collect()
}

// ── 图片代理改写（对齐原版 proxyService.rewriteList / rewriteHtml）──────────

/// 读取当前 r2Domain（原版 settingService.query().r2Domain）。
pub async fn current_r2_domain(pool: &sqlx::SqlitePool) -> String {
    sqlx::query_scalar::<_, Option<String>>("SELECT r2_domain FROM setting WHERE id=1")
        .fetch_one(pool)
        .await
        .ok()
        .flatten()
        .unwrap_or_default()
}

/// 批量改写列表里每封邮件的 content（不写回数据库），只查一次 r2Domain。
pub async fn rewrite_list_content(
    state: &std::sync::Arc<crate::models::AppState>,
    list: &mut [Value],
) {
    if list.is_empty() {
        return;
    }
    let r2_domain = current_r2_domain(&state.pool).await;
    let exp = (chrono::Utc::now().timestamp() + 3600).to_string();
    for item in list.iter_mut() {
        if let Some(obj) = item.as_object_mut() {
            if let Some(html) = obj.get("content").and_then(|v| v.as_str()) {
                let rewritten = rewrite_html(html, &state.config.jwt_secret, &r2_domain, &exp);
                obj.insert("content".into(), Value::String(rewritten));
            }
        }
    }
}

/// 把邮件 HTML 中外链图片改为带签名的同源 `/proxy?url=`。
/// 覆盖 img/source/video/audio/track/input 的地址属性、srcset 与 CSS url()，
/// 并移除 0 宽/高或 1x1 的追踪图。跳过 CID / data / blob / 本站附件 / 自有 R2。
pub fn rewrite_html(html: &str, secret: &str, r2_domain: &str, exp: &str) -> String {
    let out = remove_tracking_images(html, r2_domain);
    let out = rewrite_attrs(&out, secret, r2_domain, exp);
    let out = rewrite_srcsets(&out, secret, r2_domain, exp);
    rewrite_css_urls(&out, secret, r2_domain, exp)
}

fn rewrite_attrs(html: &str, secret: &str, r2_domain: &str, exp: &str) -> String {
    // Rust regex 不支持反向引用，用两种引号分支代替 `(["'])(.*?)\2`。
    let re = match regex::Regex::new(
        r#"(?i)\b(src|poster|background|data-src|data-original|data-lazy-src)\s*=\s*(?:"([^"]*)"|'([^']*)')"#,
    ) {
        Ok(v) => v,
        Err(_) => return html.to_string(),
    };
    re.replace_all(html, |caps: &regex::Captures| {
        let (url, quote) = match caps.get(2) {
            Some(m) => (m.as_str(), "\""),
            None => match caps.get(3) {
                Some(m) => (m.as_str(), "'"),
                None => return caps[0].to_string(),
            },
        };
        match proxy_target(url, r2_domain) {
            Some(target) => format!(
                "{}={}{}{}",
                &caps[1], quote, to_proxy_path(&target, exp, secret), quote
            ),
            None => caps[0].to_string(),
        }
    })
    .to_string()
}

fn rewrite_srcsets(html: &str, secret: &str, r2_domain: &str, exp: &str) -> String {
    let re = match regex::Regex::new(r#"(?i)\bsrcset\s*=\s*(?:"([^"]*)"|'([^']*)')"#) {
        Ok(v) => v,
        Err(_) => return html.to_string(),
    };
    re.replace_all(html, |caps: &regex::Captures| {
        let (value, quote) = match caps.get(1) {
            Some(m) => (m.as_str(), "\""),
            None => match caps.get(2) {
                Some(m) => (m.as_str(), "'"),
                None => return caps[0].to_string(),
            },
        };
        let rewritten = value
            .split(',')
            .map(|part| {
                let mut bits: Vec<String> = part
                    .trim()
                    .split_whitespace()
                    .map(|s| s.to_string())
                    .collect();
                if bits.is_empty() {
                    return part.trim().to_string();
                }
                if let Some(target) = proxy_target(&bits[0], r2_domain) {
                    bits[0] = to_proxy_path(&target, exp, secret);
                }
                bits.join(" ")
            })
            .collect::<Vec<_>>()
            .join(", ");
        format!("srcset={}{}{}", quote, rewritten, quote)
    })
    .to_string()
}

fn rewrite_css_urls(css: &str, secret: &str, r2_domain: &str, exp: &str) -> String {
    let re = match regex::Regex::new(r#"(?i)url\(\s*(?:"([^"]*)"|'([^']*)'|([^)]*))\s*\)"#) {
        Ok(v) => v,
        Err(_) => return css.to_string(),
    };
    re.replace_all(css, |caps: &regex::Captures| {
        let (url, quote) = if let Some(m) = caps.get(1) {
            (m.as_str(), "\"")
        } else if let Some(m) = caps.get(2) {
            (m.as_str(), "'")
        } else if let Some(m) = caps.get(3) {
            (m.as_str(), "")
        } else {
            return caps[0].to_string();
        };
        match proxy_target(url, r2_domain) {
            Some(target) => {
                format!("url({}{}{})", quote, to_proxy_path(&target, exp, secret), quote)
            }
            None => caps[0].to_string(),
        }
    })
    .to_string()
}

/// 移除 0 宽/高或 1x1 的外链追踪图（picture 整块一起移除）。
fn remove_tracking_images(html: &str, r2_domain: &str) -> String {
    let pic_re = match regex::Regex::new(r"(?is)<picture\b[^>]*>.*?</picture>") {
        Ok(v) => v,
        Err(_) => return html.to_string(),
    };
    let out = pic_re
        .replace_all(html, |caps: &regex::Captures| {
            if caps[0].contains("<img") && tracking_img(&caps[0], r2_domain) {
                String::new()
            } else {
                caps[0].to_string()
            }
        })
        .to_string();
    let img_re = match regex::Regex::new(r"(?is)<img\b[^>]*>") {
        Ok(v) => v,
        Err(_) => return out,
    };
    img_re
        .replace_all(&out, |caps: &regex::Captures| {
            if tracking_img(&caps[0], r2_domain) {
                String::new()
            } else {
                caps[0].to_string()
            }
        })
        .to_string()
}

fn tracking_img(tag: &str, r2_domain: &str) -> bool {
    let has_external = regex::Regex::new(
        r#"(?i)\b(src|poster|background|data-src|data-original|data-lazy-src)\s*=\s*(?:"([^"]*)"|'([^']*)')"#,
    )
    .ok()
    .map(|re| {
        re.captures_iter(tag).any(|c| {
            c.get(2)
                .or_else(|| c.get(3))
                .map(|m| proxy_target(m.as_str(), r2_domain).is_some())
                .unwrap_or(false)
        })
    })
    .unwrap_or(false)
        || regex::Regex::new(r#"(?i)\bsrcset\s*=\s*(?:"([^"]*)"|'([^']*)')"#)
            .ok()
            .map(|re| {
                re.captures_iter(tag).any(|c| {
                    let value = c.get(1).or_else(|| c.get(2));
                    value.map(|v| {
                        v.as_str().split(',').filter_map(|p| p.trim().split_whitespace().next())
                            .any(|u| proxy_target(u, r2_domain).is_some())
                    }).unwrap_or(false)
                })
            })
            .unwrap_or(false);
    if !has_external {
        return false;
    }
    let dim = |name: &str| -> Option<i64> {
        let re = regex::Regex::new(&format!(r#"(?i)\b{name}\s*=\s*["']?([^"'\s>]+)"#)).ok()?;
        let raw = re.captures(tag)?.get(1)?.as_str();
        let value = raw.trim_end_matches("px").trim_end_matches('%');
        value.parse::<i64>().ok()
    };
    let style_dim = |name: &str| -> Option<i64> {
        let re = regex::Regex::new(&format!(r"(?i)(?:^|[;\s]){name}\s*:\s*([0-9.]+)(px|%)?")).ok()?;
        let raw = re.captures(tag)?.get(1)?.as_str();
        raw.parse::<f64>().ok().map(|v| v as i64)
    };
    let width = dim("width").or_else(|| style_dim("width"));
    let height = dim("height").or_else(|| style_dim("height"));
    width == Some(0)
        || height == Some(0)
        || (width == Some(1) && height == Some(1))
}

/// 判断一个 URL 是否应被代理；返回签名用的绝对 URL。
fn proxy_target(raw: &str, r2_domain: &str) -> Option<String> {
    let value = raw.trim();
    if value.is_empty() || value.starts_with("{{domain}}") {
        return None;
    }
    // 已代理地址：还原原始 URL 后用新 exp 重新签名（原版 rewriter 兼容旧链接）
    if let Some(original) = from_proxy_path(value) {
        return Some(original);
    }
    let lower = value.to_lowercase();
    if lower.starts_with("data:") || lower.starts_with("blob:") || lower.starts_with("cid:") {
        return None;
    }
    if value.contains("proxy?url=") {
        return None;
    }
    let value = if let Some(rest) = value.strip_prefix("//") {
        format!("https:{rest}")
    } else {
        value.to_string()
    };
    let url = reqwest::Url::parse(&value).ok()?;
    if url.scheme() != "http" && url.scheme() != "https" {
        return None;
    }
    let own_origin = r2_origin(r2_domain);
    if !own_origin.is_empty() && url.origin().ascii_serialization() == own_origin {
        return None;
    }
    Some(value)
}

fn from_proxy_path(raw: &str) -> Option<String> {
    if !raw.contains("proxy?") {
        return None;
    }
    let parsed = if raw.starts_with("http://") || raw.starts_with("https://") {
        reqwest::Url::parse(raw).ok()?
    } else {
        reqwest::Url::parse(&format!("https://proxy.invalid/{}", raw.trim_start_matches('/'))).ok()?
    };
    if parsed.path() != "/proxy" {
        return None;
    }
    let original = parsed
        .query_pairs()
        .find(|(k, _)| k == "url")
        .map(|(_, v)| v.to_string())?;
    if original.starts_with("http://") || original.starts_with("https://") {
        Some(original)
    } else {
        None
    }
}

fn r2_origin(domain: &str) -> String {
    if domain.is_empty() {
        return String::new();
    }
    let with_scheme = if domain.starts_with("http://") || domain.starts_with("https://") {
        domain.to_string()
    } else {
        format!("https://{domain}")
    };
    reqwest::Url::parse(&with_scheme)
        .map(|u| u.origin().ascii_serialization())
        .unwrap_or_default()
}

fn to_proxy_path(url: &str, exp: &str, secret: &str) -> String {
    let sig = crate::utils::proxy_sign(url, exp, secret);
    format!(
        "/proxy?url={}&exp={}&sig={}",
        form_urlencode(url),
        exp,
        sig
    )
}

/// 对齐原版 URLSearchParams 的编码（空格为 `+`，其余非 unreserved 百分号编码）。
fn form_urlencode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}
