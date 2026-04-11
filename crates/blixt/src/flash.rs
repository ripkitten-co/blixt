use axum::extract::FromRequestParts;
use axum::http::header::{COOKIE, LOCATION, SET_COOKIE};
use axum::http::request::Parts;
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};

static FLASH_COOKIE_NAME: &str = "blixt_flash";

/// Flash message severity level.
#[derive(Debug, Clone)]
pub enum FlashLevel {
    /// Operation succeeded.
    Success,
    /// Operation failed.
    Error,
    /// Informational notice.
    Info,
}

/// A cookie-based read-once message for redirect-after-POST flows.
#[derive(Debug, Clone)]
pub struct Flash {
    level: FlashLevel,
    message: String,
}

impl Flash {
    /// Create a success flash.
    pub fn success(message: &str) -> Self {
        Self {
            level: FlashLevel::Success,
            message: message.to_owned(),
        }
    }

    /// Create an error flash.
    pub fn error(message: &str) -> Self {
        Self {
            level: FlashLevel::Error,
            message: message.to_owned(),
        }
    }

    /// Create an info flash.
    pub fn info(message: &str) -> Self {
        Self {
            level: FlashLevel::Info,
            message: message.to_owned(),
        }
    }

    /// The flash message text.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// True if this is a success flash.
    pub fn is_success(&self) -> bool {
        matches!(self.level, FlashLevel::Success)
    }

    /// True if this is an error flash.
    pub fn is_error(&self) -> bool {
        matches!(self.level, FlashLevel::Error)
    }

    /// True if this is an info flash.
    pub fn is_info(&self) -> bool {
        matches!(self.level, FlashLevel::Info)
    }

    fn to_cookie_value(&self) -> String {
        let level = match self.level {
            FlashLevel::Success => "success",
            FlashLevel::Error => "error",
            FlashLevel::Info => "info",
        };
        format!("{level}:{}", self.message)
    }

    fn from_cookie_value(value: &str) -> Option<Self> {
        let (level_str, message) = value.split_once(':')?;
        let level = match level_str {
            "success" => FlashLevel::Success,
            "error" => FlashLevel::Error,
            "info" => FlashLevel::Info,
            _ => return None,
        };
        Some(Self {
            level,
            message: message.to_owned(),
        })
    }
}

impl<S: Send + Sync> FromRequestParts<S> for Flash {
    type Rejection = StatusCode;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> std::result::Result<Self, Self::Rejection> {
        let cookie_header = parts.headers.get(COOKIE).and_then(|v| v.to_str().ok());
        let flash_value = cookie_header.and_then(|cookies| {
            cookies.split(';').map(str::trim).find_map(|pair| {
                let (name, value) = pair.split_once('=')?;
                if name.trim() == FLASH_COOKIE_NAME {
                    Some(urlencoding::decode(value.trim()).ok()?.into_owned())
                } else {
                    None
                }
            })
        });

        match flash_value.and_then(|v| Flash::from_cookie_value(&v)) {
            Some(flash) => Ok(flash),
            None => Err(StatusCode::NOT_FOUND),
        }
    }
}

/// HTTP redirect response with optional flash message.
pub struct Redirect {
    location: String,
    flash: Option<Flash>,
}

impl Redirect {
    /// Create a redirect to the given path.
    pub fn to(location: &str) -> Self {
        Self {
            location: location.to_owned(),
            flash: None,
        }
    }

    /// Attach a flash message to the redirect.
    pub fn with_flash(mut self, flash: Flash) -> Self {
        self.flash = Some(flash);
        self
    }
}

impl IntoResponse for Redirect {
    fn into_response(self) -> Response {
        let mut response = StatusCode::SEE_OTHER.into_response();
        if let Ok(loc) = HeaderValue::from_str(&self.location) {
            response.headers_mut().insert(LOCATION, loc);
        }
        if let Some(flash) = self.flash {
            let value = flash.to_cookie_value();
            let encoded = urlencoding::encode(&value);
            let cookie = format!(
                "{FLASH_COOKIE_NAME}={encoded}; Path=/; HttpOnly; SameSite=Lax; Max-Age=60"
            );
            if let Ok(val) = HeaderValue::from_str(&cookie) {
                response.headers_mut().append(SET_COOKIE, val);
            }
        }
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flash_success_has_correct_level() {
        let f = Flash::success("done");
        assert!(f.is_success());
        assert!(!f.is_error());
        assert_eq!(f.message(), "done");
    }

    #[test]
    fn flash_error_has_correct_level() {
        let f = Flash::error("failed");
        assert!(f.is_error());
        assert!(!f.is_success());
        assert_eq!(f.message(), "failed");
    }

    #[test]
    fn flash_info_has_correct_level() {
        let f = Flash::info("note");
        assert!(f.is_info());
        assert_eq!(f.message(), "note");
    }

    #[test]
    fn flash_cookie_roundtrip() {
        let f = Flash::success("Post created");
        let cookie = f.to_cookie_value();
        assert_eq!(cookie, "success:Post created");
        let parsed = Flash::from_cookie_value(&cookie).unwrap();
        assert!(parsed.is_success());
        assert_eq!(parsed.message(), "Post created");
    }

    #[test]
    fn flash_parse_handles_colons_in_message() {
        let f = Flash::from_cookie_value("info:Time: 12:30").unwrap();
        assert!(f.is_info());
        assert_eq!(f.message(), "Time: 12:30");
    }

    #[test]
    fn flash_parse_rejects_invalid_format() {
        assert!(Flash::from_cookie_value("garbage").is_none());
        assert!(Flash::from_cookie_value("").is_none());
        assert!(Flash::from_cookie_value("unknown:msg").is_none());
    }

    #[tokio::test]
    async fn redirect_returns_303_with_location() {
        let redirect = Redirect::to("/posts");
        let response = redirect.into_response();
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert_eq!(response.headers().get("location").unwrap(), "/posts");
    }

    #[tokio::test]
    async fn redirect_with_flash_sets_cookie() {
        let redirect = Redirect::to("/posts").with_flash(Flash::success("Created"));
        let response = redirect.into_response();
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        let cookie = response
            .headers()
            .get("set-cookie")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(cookie.contains("blixt_flash=success%3ACreated"));
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("SameSite=Lax"));
    }

    #[tokio::test]
    async fn redirect_without_flash_has_no_cookie() {
        let redirect = Redirect::to("/home");
        let response = redirect.into_response();
        assert!(response.headers().get("set-cookie").is_none());
    }
}
