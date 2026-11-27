use std::sync::Arc;
use sqlx::SqlitePool;

use crate::config::Config;
use crate::rate_limit::IpRateLimiter;

pub struct AppState {
    pub pool: SqlitePool,
    pub config: Arc<Config>,
    pub rate_limiter: Arc<IpRateLimiter>,
}
