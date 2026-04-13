use axum::http::HeaderValue;
use axum::http::header::SET_COOKIE;
use axum::response::Response;

/// Name of the HttpOnly cookie used for session authentication.
pub static AUTH_COOKIE_NAME: &str = "blixt_auth";

/// Sets the auth cookie on a response with the given JWT token.
///
/// Cookie flags: HttpOnly, SameSite=Strict, Path=/. Secure flag is added when
/// `secure` is true (should be true in production).
pub fn set(response: &mut Response, token: &str, max_age_secs: u64, secure: bool) {
    let secure_flag = if secure { "; Secure" } else { "" };
    let cookie = format!(
        "{AUTH_COOKIE_NAME}={token}; Path=/; HttpOnly; SameSite=Strict; Max-Age={max_age_secs}{secure_flag}"
    );
    match HeaderValue::from_str(&cookie) {
        Ok(val) => {
            response.headers_mut().append(SET_COOKIE, val);
        }
        Err(_) => {
            tracing::warn!("failed to set auth cookie: invalid header value");
        }
    }
}

/// Clears the auth cookie by setting it to empty with Max-Age=0.
pub fn clear(response: &mut Response) {
    let cookie = format!("{AUTH_COOKIE_NAME}=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0");
    if let Ok(val) = HeaderValue::from_str(&cookie) {
        response.headers_mut().append(SET_COOKIE, val);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use axum::response::IntoResponse;

    fn blank_response() -> Response {
        StatusCode::OK.into_response()
    }

    #[test]
    fn set_cookie_adds_httponly_header() {
        let mut resp = blank_response();
        set(&mut resp, "jwt-token-here", 3600, false);

        let cookie = resp
            .headers()
            .get(SET_COOKIE)
            .expect("set-cookie header")
            .to_str()
            .expect("valid str");
        assert!(cookie.contains("blixt_auth=jwt-token-here"));
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("SameSite=Strict"));
        assert!(cookie.contains("Max-Age=3600"));
        assert!(cookie.contains("Path=/"));
        assert!(!cookie.contains("Secure"));
    }

    #[test]
    fn set_cookie_with_secure_flag() {
        let mut resp = blank_response();
        set(&mut resp, "token", 7200, true);

        let cookie = resp
            .headers()
            .get(SET_COOKIE)
            .expect("set-cookie header")
            .to_str()
            .expect("valid str");
        assert!(cookie.contains("Secure"));
    }

    #[test]
    fn clear_cookie_sets_max_age_zero() {
        let mut resp = blank_response();
        clear(&mut resp);

        let cookie = resp
            .headers()
            .get(SET_COOKIE)
            .expect("set-cookie header")
            .to_str()
            .expect("valid str");
        assert!(cookie.contains("blixt_auth=;"));
        assert!(cookie.contains("Max-Age=0"));
    }
}
