#![cfg(test)]

use crate::config::{Config, Environment};

pub fn test_config() -> Config {
    Config {
        host: "127.0.0.1".to_string(),
        port: 0,
        blixt_env: Environment::Test,
        database_url: None,
        jwt_secret: None,
    }
}

pub fn test_db_url() -> Option<String> {
    std::env::var("TEST_DATABASE_URL").ok()
}

macro_rules! require_db {
    () => {
        if $crate::test_helpers::test_db_url().is_none() {
            eprintln!("skipping: TEST_DATABASE_URL not set");
            return;
        }
    };
}

pub(crate) use require_db;
