pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

pub fn now_secs() -> i64 {
    chrono::Utc::now().timestamp()
}

pub const DAY_MS: i64 = 86_400_000;