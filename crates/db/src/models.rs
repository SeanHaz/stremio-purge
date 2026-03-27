use serde::{Deserialize, Serialize};
use sqlx::FromRow;

pub const SYNC_SERIES: i64 = 1 << 0;
pub const SYNC_MOVIES: i64 = 1 << 1;
pub const SYNC_TV: i64 = 1 << 2;
pub const SYNC_ALL: i64 = SYNC_MOVIES | SYNC_SERIES | SYNC_TV;

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct User {
    pub id: String,
    pub auth_key: String,
    pub all_timestamp: i64,
    pub series_timestamp: i64,
    pub movies_timestamp: i64,
    pub tv_timestamp: i64,
    pub config_mask: i64,
} 

impl User {
    pub fn new(
        id: impl Into<String>,
        auth_key: impl Into<String>,
        all_timestamp: i64,
        series_timestamp: i64,
        movies_timestamp: i64,
        tv_timestamp: i64,
        config_mask: i64,
    ) -> Self {
        Self {
            id: id.into(),
            auth_key: auth_key.into(),
            all_timestamp,
            series_timestamp,
            movies_timestamp,
            tv_timestamp,
            config_mask,
        }
    }
    
    pub fn is_active(&self) -> bool {
        self.config_mask != 0
    }

    pub fn is_bit_active(&self, bit: i64) -> bool {
        self.config_mask & bit != 0
    }

    pub fn is_all_active(&self) -> bool {
        self.config_mask == SYNC_ALL
    }

    pub fn get_min_active(&self) -> i64 {
        let mut targets = Vec::new();

        if self.is_bit_active(SYNC_SERIES) {
            targets.push(self.series_timestamp);
        }
        if self.is_bit_active(SYNC_MOVIES) {
            targets.push(self.movies_timestamp);
        }
        if self.is_bit_active(SYNC_TV) {
            targets.push(self.tv_timestamp);
        }

        let min_found = targets.into_iter().min().unwrap_or(i64::MAX);
        min_found.max(self.all_timestamp)
    }

    pub fn update_active_timestamps(&mut self, timestamp: i64) {
        if self.config_mask == SYNC_ALL {
            self.all_timestamp = timestamp;
            return
        }
        if self.is_bit_active(SYNC_SERIES) {
            self.series_timestamp = timestamp;
        };
        if self.is_bit_active(SYNC_MOVIES) {

            self.movies_timestamp = timestamp;
        };
        if self.is_bit_active(SYNC_TV) {
            self.tv_timestamp = timestamp;
        };
    }
}
