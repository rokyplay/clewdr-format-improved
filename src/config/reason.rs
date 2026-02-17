use std::{
    fmt::{Debug, Display},
    hash::Hash,
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::{CookieStatus, UsageBreakdown};
use crate::config::ClewdrCookie;

/// Reason why a cookie is considered useless
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash, Error)]
pub enum Reason {
    NormalPro,
    Free,
    Disabled,
    Banned,
    Null,
    Restricted(i64),
    TooManyRequest(i64),
}

impl Display for Reason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let format_time = |secs: i64| {
            chrono::DateTime::from_timestamp(secs, 0)
                .map(|t| t.format("UTC %Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or("Invalid date".to_string())
        };
        match self {
            Reason::NormalPro => write!(f, "Normal Pro account"),
            Reason::Disabled => write!(f, "Organization Disabled"),
            Reason::Free => write!(f, "Free account"),
            Reason::Banned => write!(f, "Banned"),
            Reason::Null => write!(f, "Null"),
            Reason::Restricted(i) => {
                write!(f, "Restricted/Warning: until {}", format_time(*i))
            }
            Reason::TooManyRequest(i) => {
                write!(f, "429 Too many request: until {}", format_time(*i))
            }
        }
    }
}

/// A struct representing a cookie that can't be used
/// Contains the cookie and the reason why it's considered unusable
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UselessCookie {
    pub cookie: ClewdrCookie,
    pub reason: Reason,

    // Analytics: preserved from CookieStatus when invalidated
    #[serde(default)]
    pub added_at: Option<i64>,
    #[serde(default)]
    pub invalidated_at: Option<i64>,
    #[serde(default)]
    pub first_request_at: Option<i64>,
    #[serde(default)]
    pub last_request_at: Option<i64>,
    #[serde(default)]
    pub request_count: u64,
    #[serde(default)]
    pub lifetime_usage: UsageBreakdown,
}

impl PartialEq<CookieStatus> for UselessCookie {
    fn eq(&self, other: &CookieStatus) -> bool {
        self.cookie == other.cookie
    }
}

impl PartialEq for UselessCookie {
    fn eq(&self, other: &Self) -> bool {
        self.cookie == other.cookie
    }
}

impl Eq for UselessCookie {}

impl Hash for UselessCookie {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.cookie.hash(state);
    }
}

impl UselessCookie {
    /// Creates a new UselessCookie instance (legacy, without analytics)
    ///
    /// # Arguments
    /// * `cookie` - The cookie that is unusable
    /// * `reason` - The reason why the cookie is unusable
    ///
    /// # Returns
    /// A new UselessCookie instance
    pub fn new(cookie: ClewdrCookie, reason: Reason) -> Self {
        Self {
            cookie,
            reason,
            added_at: None,
            invalidated_at: None,
            first_request_at: None,
            last_request_at: None,
            request_count: 0,
            lifetime_usage: UsageBreakdown::default(),
        }
    }

    /// Creates a UselessCookie from a CookieStatus, preserving analytics data
    pub fn from_cookie_status(status: &CookieStatus, reason: Reason) -> Self {
        Self {
            cookie: status.cookie.clone(),
            reason,
            added_at: status.added_at,
            invalidated_at: Some(chrono::Utc::now().timestamp()),
            first_request_at: status.first_request_at,
            last_request_at: status.last_request_at,
            request_count: status.request_count,
            lifetime_usage: status.lifetime_usage.clone(),
        }
    }
}
