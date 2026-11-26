use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;

/// 全部配置来自 TOML 文件；命令行用 `-c <file>` 指定，未指定时读取当前目录的 config.toml。
/// 字段缺省时使用下面的 Default，因此不写任何配置文件也能启动。
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    pub host: String,
    pub port: u16,
    /// SQLite 数据库所在目录，启动时自动创建（无需手动 mkdir）
    pub database_dir: String,
    /// 完整数据库连接串；留空时由 database_dir 自动生成
    pub database_url: Option<String>,
    /// SMTP 会话横幅中的主机名
    pub smtp_hostname: String,
    pub storage_dir: String,
    pub jwt_secret: String,
    pub domains: Vec<String>,
    pub admin: String,
    pub cors_origins: Vec<String>,
    pub smtp: Option<SmtpConfig>,
    pub smtp_listen_port: u16,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub from: String,
    pub tls: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".into(),
            port: 8080,
            database_dir: "data".into(),
            database_url: None,
            smtp_hostname: "cloud-mail".into(),
            storage_dir: "data/storage".into(),
            // 生产环境请在配置文件中覆盖为随机长字符串
            jwt_secret: "cloudmail-default-secret-change-me".into(),
            domains: vec!["example.com".into()],
            admin: "admin@example.com".into(),
            cors_origins: vec!["http://localhost:3001".into()],
            smtp: None,
            smtp_listen_port: 25,
        }
    }
}

impl Default for SmtpConfig {
    fn default() -> Self {
        Self {
            host: String::new(),
            port: 587,
            user: String::new(),
            password: String::new(),
            from: String::new(),
            tls: true,
        }
    }
}

impl Config {
    /// 解析命令行：支持 `-c file`、`--config file`、`--config=file`。
    /// 没有 -c 时，若当前目录存在 config.toml 则读取，否则使用默认配置。
    pub fn from_args() -> Result<Self> {
        let mut path: Option<String> = None;
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            if arg == "-c" || arg == "--config" {
                path = Some(args.next().context("-c/--config 缺少配置文件路径")?);
            } else if let Some(value) = arg.strip_prefix("--config=") {
                path = Some(value.to_string());
            } else {
                anyhow::bail!("未知参数: {arg}；用法: cloudmail [-c <config.toml>]");
            }
        }

        match path {
            Some(p) => Self::from_file(p),
            None => {
                let default_path = Path::new("config.toml");
                if default_path.is_file() {
                    Self::from_file(default_path)
                } else {
                    Self::default().finalize()
                }
            }
        }
    }

    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("读取配置文件失败: {}", path.display()))?;
        let cfg: Self = toml::from_str(&text)
            .with_context(|| format!("解析配置文件失败: {}", path.display()))?;
        cfg.finalize()
    }

    fn finalize(mut self) -> Result<Self> {
        if self.database_dir.trim().is_empty() {
            anyhow::bail!("database_dir 不能为空");
        }
        if self.database_url.is_none() {
            let dir = self
                .database_dir
                .trim_end_matches('/')
                .trim_start_matches("./");
            self.database_url = Some(format!("sqlite://{dir}/cloud-mail.db?mode=rwc"));
        }
        // [smtp] 存在但 host 为空 => 视为未配置发信，与原 SMTP_HOST 行为一致
        if self
            .smtp
            .as_ref()
            .is_some_and(|s| s.host.trim().is_empty())
        {
            self.smtp = None;
        }
        Ok(self)
    }

    pub fn database_url(&self) -> &str {
        self.database_url
            .as_deref()
            .unwrap_or("sqlite://data/cloud-mail.db?mode=rwc")
    }

    pub fn domain_allowed(&self, email: &str) -> bool {
        self.domains.iter().any(|domain| {
            email.to_lowercase().ends_with(&format!("@{}", domain.to_lowercase()))
        })
    }
}
