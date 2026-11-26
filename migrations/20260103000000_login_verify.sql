-- Turnstile 登录验证：对齐原版 v3_6 迁移新增的两列。
-- loginVerify: 0=OPEN(每次都验证) 1=CLOSE 2=COUNT(达到次数后验证)
ALTER TABLE setting ADD COLUMN login_verify INTEGER NOT NULL DEFAULT 1;
ALTER TABLE setting ADD COLUMN login_verify_count INTEGER NOT NULL DEFAULT 1;
