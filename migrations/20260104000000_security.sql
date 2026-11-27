-- 安全机制：登录失败计数 / 临时锁定（按 IP 与邮箱双维度）
CREATE TABLE IF NOT EXISTS login_attempt (
    ip          TEXT NOT NULL,
    email       TEXT NOT NULL COLLATE NOCASE,
    fail_count  INTEGER NOT NULL DEFAULT 0,
    locked_until TEXT,
    PRIMARY KEY (ip, email)
);
CREATE INDEX IF NOT EXISTS idx_login_attempt_ip ON login_attempt(ip);
CREATE INDEX IF NOT EXISTS idx_login_attempt_email ON login_attempt(email);
