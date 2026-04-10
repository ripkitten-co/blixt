use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use std::fmt;

/// Runtime environment.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Environment {
    /// Local development (default).
    Development,
    /// Production deployment.
    Production,
    /// Automated test runs.
    Test,
}

impl Environment {
    fn from_env_var() -> Self {
        match std::env::var("BLIXT_ENV").as_deref() {
            Ok("production") => Self::Production,
            Ok("test") => Self::Test,
            _ => Self::Development,
        }
    }
}

/// Application configuration loaded from environment variables.
#[derive(Clone)]
pub struct Config {
    /// Bind address (default `127.0.0.1`, env `HOST`).
    pub host: String,
    /// Listen port (default `3000`, env `PORT`).
    pub port: u16,
    /// Runtime environment (env `BLIXT_ENV`).
    pub blixt_env: Environment,
    /// Database connection string (env `DATABASE_URL`).
    pub database_url: Option<SecretString>,
    /// HMAC secret for JWT signing (env `JWT_SECRET`).
    pub jwt_secret: Option<SecretString>,
}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("blixt_env", &self.blixt_env)
            .field("database_url", &"[REDACTED]")
            .field("jwt_secret", &"[REDACTED]")
            .finish()
    }
}

impl Config {
    /// Loads configuration from environment variables and `.env` files.
    ///
    /// In non-production environments, `.env` is loaded via dotenvy first.
    pub fn from_env() -> crate::error::Result<Self> {
        let blixt_env = Environment::from_env_var();

        if blixt_env != Environment::Production {
            dotenvy::dotenv().ok();
        }

        let host = std::env::var("HOST").unwrap_or_else(|_| default_host());

        if blixt_env == Environment::Production && host == "0.0.0.0" {
            tracing::warn!(
                "host is bound to 0.0.0.0 in production — this exposes the \
                 server on all network interfaces"
            );
        }

        let config = Self {
            host,
            port: std::env::var("PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or_else(default_port),
            blixt_env,
            database_url: std::env::var("DATABASE_URL").ok().map(SecretString::from),
            jwt_secret: std::env::var("JWT_SECRET").ok().map(SecretString::from),
        };
        Ok(config)
    }

    /// Exposes the database URL for pool creation.
    pub fn database_url(&self) -> Option<&str> {
        self.database_url.as_ref().map(|s| s.expose_secret())
    }

    /// Exposes the JWT secret for token signing.
    pub fn jwt_secret(&self) -> Option<&str> {
        self.jwt_secret.as_ref().map(|s| s.expose_secret())
    }
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    3000
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// # Safety
    /// Callers must ensure no other threads read env vars concurrently.
    /// We enforce this via ENV_LOCK.
    fn with_env_vars<F, R>(vars: &[(&str, Option<&str>)], f: F) -> R
    where
        F: FnOnce() -> R,
    {
        let _guard = ENV_LOCK.lock().expect("env lock poisoned");

        let mut previous: Vec<(&str, Option<String>)> = Vec::new();
        for &(key, value) in vars {
            previous.push((key, std::env::var(key).ok()));
            // SAFETY: protected by ENV_LOCK mutex; tests run serially
            unsafe {
                match value {
                    Some(v) => std::env::set_var(key, v),
                    None => std::env::remove_var(key),
                }
            }
        }

        let result = f();

        for (key, prev) in previous {
            // SAFETY: protected by ENV_LOCK mutex; restoring original values
            unsafe {
                match prev {
                    Some(v) => std::env::set_var(key, v),
                    None => std::env::remove_var(key),
                }
            }
        }

        result
    }

    #[test]
    fn debug_output_redacts_secrets() {
        let config = Config {
            host: "localhost".to_string(),
            port: 3000,
            blixt_env: Environment::Development,
            database_url: Some(SecretString::from(
                "postgres://user:password@localhost/db".to_string(),
            )),
            jwt_secret: Some(SecretString::from("super-secret-jwt-key".to_string())),
        };

        let debug_output = format!("{:?}", config);

        assert!(
            !debug_output.contains("postgres://"),
            "debug output must not contain the database URL"
        );
        assert!(
            !debug_output.contains("password"),
            "debug output must not contain password"
        );
        assert!(
            !debug_output.contains("super-secret-jwt-key"),
            "debug output must not contain the JWT secret"
        );
        assert!(
            debug_output.contains("[REDACTED]"),
            "debug output must show [REDACTED] for secrets"
        );
    }

    #[test]
    fn production_does_not_load_dotenv() {
        with_env_vars(
            &[
                ("BLIXT_ENV", Some("production")),
                ("JWT_SECRET", Some("prod-jwt-key")),
                ("DATABASE_URL", None),
                ("HOST", None),
                ("PORT", None),
            ],
            || {
                let config = Config::from_env().expect("config should load");
                assert_eq!(config.blixt_env, Environment::Production);
                assert_eq!(config.host, default_host());
                assert_eq!(config.port, default_port());
            },
        );
    }

    #[test]
    fn defaults_to_development_when_blixt_env_unset() {
        with_env_vars(
            &[
                ("BLIXT_ENV", None),
                ("JWT_SECRET", Some("dev-key")),
                ("DATABASE_URL", None),
                ("HOST", None),
                ("PORT", None),
            ],
            || {
                let config = Config::from_env().expect("config should load");
                assert_eq!(config.blixt_env, Environment::Development);
            },
        );
    }

    #[test]
    fn empty_blixt_env_defaults_to_development() {
        with_env_vars(
            &[
                ("BLIXT_ENV", Some("")),
                ("JWT_SECRET", None),
                ("DATABASE_URL", None),
                ("HOST", None),
                ("PORT", None),
            ],
            || {
                assert_eq!(Environment::from_env_var(), Environment::Development);
            },
        );
    }

    #[test]
    fn expose_secret_accessors_return_values() {
        let config = Config {
            host: "localhost".to_string(),
            port: 8080,
            blixt_env: Environment::Test,
            database_url: Some(SecretString::from("postgres://test".to_string())),
            jwt_secret: Some(SecretString::from("jwt-test".to_string())),
        };

        assert_eq!(config.database_url(), Some("postgres://test"));
        assert_eq!(config.jwt_secret(), Some("jwt-test"));
    }

    #[test]
    fn environment_from_env_var_parses_variants() {
        with_env_vars(&[("BLIXT_ENV", Some("production"))], || {
            assert_eq!(Environment::from_env_var(), Environment::Production);
        });

        with_env_vars(&[("BLIXT_ENV", Some("test"))], || {
            assert_eq!(Environment::from_env_var(), Environment::Test);
        });

        with_env_vars(&[("BLIXT_ENV", Some("development"))], || {
            assert_eq!(Environment::from_env_var(), Environment::Development);
        });

        with_env_vars(&[("BLIXT_ENV", Some("unknown"))], || {
            assert_eq!(Environment::from_env_var(), Environment::Development);
        });
    }
}
