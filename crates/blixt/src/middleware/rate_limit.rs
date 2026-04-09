use axum::extract::ConnectInfo;
use axum::http::{HeaderValue, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Token-bucket rate limiter keyed by client IP address.
///
/// Each IP receives `max_requests` tokens that replenish at a constant rate
/// over `window_secs`. Exceeding the budget returns 429 Too Many Requests.
///
/// Thread-safe via `Arc<Mutex<...>>` — suitable for typical web workloads.
#[derive(Debug, Clone)]
pub struct RateLimiter {
    state: Arc<Mutex<HashMap<IpAddr, Bucket>>>,
    max_requests: f64,
    window_secs: f64,
}

#[derive(Debug)]
struct Bucket {
    tokens: f64,
    last_check: Instant,
}

impl RateLimiter {
    /// Creates a rate limiter with the given capacity and window.
    pub fn new(max_requests: u32, window_secs: u64) -> Self {
        Self {
            state: Arc::new(Mutex::new(HashMap::new())),
            max_requests: f64::from(max_requests),
            window_secs: window_secs as f64,
        }
    }

    /// Default rate limiter: 100 requests per 60 seconds.
    pub fn default_limit() -> Self {
        Self::new(100, 60)
    }

    /// Strict rate limiter: 10 requests per 60 seconds (for auth endpoints).
    pub fn strict() -> Self {
        Self::new(10, 60)
    }

    /// Checks whether a request from the given IP is allowed.
    ///
    /// Returns `true` if the request is within budget, `false` if rate-limited.
    pub fn check(&self, ip: IpAddr) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|poisoned| {
            tracing::error!("rate limiter mutex poisoned, recovering");
            poisoned.into_inner()
        });

        let now = Instant::now();
        let bucket = state.entry(ip).or_insert_with(|| Bucket {
            tokens: self.max_requests,
            last_check: now,
        });

        replenish_tokens(bucket, now, self.max_requests, self.window_secs);

        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    /// Returns the configured window in seconds (for Retry-After headers).
    pub fn window_secs(&self) -> u64 {
        self.window_secs as u64
    }
}

/// Replenishes tokens in a bucket based on elapsed time.
fn replenish_tokens(bucket: &mut Bucket, now: Instant, max: f64, window: f64) {
    let elapsed = now.duration_since(bucket.last_check).as_secs_f64();
    let rate = max / window;
    bucket.tokens = (bucket.tokens + elapsed * rate).min(max);
    bucket.last_check = now;
}

/// Axum middleware that enforces per-IP rate limiting.
///
/// Extracts the client IP from `ConnectInfo<SocketAddr>` or the
/// `x-forwarded-for` header as a fallback. Returns 429 with a `Retry-After`
/// header when the rate limit is exceeded.
///
/// # Usage
///
/// ```rust,ignore
/// use axum::{Router, middleware};
/// use blixt::middleware::rate_limit::{RateLimiter, rate_limit_middleware};
///
/// let limiter = RateLimiter::default_limit();
/// let app = Router::new()
///     .route("/api", get(handler))
///     .layer(middleware::from_fn_with_state(limiter, rate_limit_middleware));
/// ```
pub async fn rate_limit_middleware(
    axum::extract::State(limiter): axum::extract::State<RateLimiter>,
    connect_info: Option<ConnectInfo<SocketAddr>>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let ip = extract_client_ip(&request, connect_info);

    if limiter.check(ip) {
        next.run(request).await
    } else {
        build_rate_limited_response(limiter.window_secs())
    }
}

/// Extracts the client IP, preferring ConnectInfo and falling back to
/// x-forwarded-for, then to a loopback address.
fn extract_client_ip(
    request: &Request<axum::body::Body>,
    connect_info: Option<ConnectInfo<SocketAddr>>,
) -> IpAddr {
    if let Some(ConnectInfo(addr)) = connect_info {
        return addr.ip();
    }

    request
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .and_then(|s| s.trim().parse::<IpAddr>().ok())
        .unwrap_or(IpAddr::V4(std::net::Ipv4Addr::LOCALHOST))
}

/// Builds a 429 Too Many Requests response with a Retry-After header.
fn build_rate_limited_response(retry_after: u64) -> Response {
    let mut response = (StatusCode::TOO_MANY_REQUESTS, "Too many requests").into_response();
    if let Ok(value) = HeaderValue::from_str(&retry_after.to_string()) {
        response
            .headers_mut()
            .insert(axum::http::header::RETRY_AFTER, value);
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn allows_requests_within_limit() {
        let limiter = RateLimiter::strict();
        let ip = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));

        for i in 0..10 {
            assert!(limiter.check(ip), "request {i} should be allowed");
        }
    }

    #[test]
    fn rejects_request_exceeding_limit() {
        let limiter = RateLimiter::strict();
        let ip = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2));

        for _ in 0..10 {
            limiter.check(ip);
        }

        assert!(!limiter.check(ip), "11th request should be rejected");
    }

    #[test]
    fn different_ips_have_independent_limits() {
        let limiter = RateLimiter::strict();
        let ip_a = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 3));
        let ip_b = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 4));

        for _ in 0..10 {
            limiter.check(ip_a);
        }

        assert!(!limiter.check(ip_a), "IP A should be rate limited");
        assert!(limiter.check(ip_b), "IP B should still be allowed");
    }

    #[test]
    fn tokens_replenish_after_time_elapses() {
        let limiter = RateLimiter::new(2, 1);
        let ip = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 5));

        assert!(limiter.check(ip));
        assert!(limiter.check(ip));
        assert!(!limiter.check(ip), "bucket should be empty");

        // Simulate time passing by directly manipulating the bucket
        {
            let mut state = limiter.state.lock().expect("lock");
            let bucket = state.get_mut(&ip).expect("bucket exists");
            bucket.last_check -= std::time::Duration::from_secs(2);
        }

        assert!(
            limiter.check(ip),
            "tokens should have replenished after window elapsed"
        );
    }

    #[test]
    fn default_limit_is_100_per_60s() {
        let limiter = RateLimiter::default_limit();
        assert_eq!(limiter.max_requests as u32, 100);
        assert_eq!(limiter.window_secs(), 60);
    }

    #[test]
    fn strict_limit_is_10_per_60s() {
        let limiter = RateLimiter::strict();
        assert_eq!(limiter.max_requests as u32, 10);
        assert_eq!(limiter.window_secs(), 60);
    }

    #[test]
    fn extract_client_ip_prefers_connect_info() {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)), 8080);
        let request = Request::builder()
            .header("x-forwarded-for", "10.0.0.1")
            .body(axum::body::Body::empty())
            .expect("build request");

        let ip = extract_client_ip(&request, Some(ConnectInfo(addr)));
        assert_eq!(ip, IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)));
    }

    #[test]
    fn extract_client_ip_falls_back_to_forwarded_for() {
        let request = Request::builder()
            .header("x-forwarded-for", "10.0.0.1, 172.16.0.1")
            .body(axum::body::Body::empty())
            .expect("build request");

        let ip = extract_client_ip(&request, None);
        assert_eq!(ip, IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)));
    }

    #[test]
    fn extract_client_ip_defaults_to_localhost() {
        let request = Request::builder()
            .body(axum::body::Body::empty())
            .expect("build request");

        let ip = extract_client_ip(&request, None);
        assert_eq!(ip, IpAddr::V4(Ipv4Addr::LOCALHOST));
    }
}
