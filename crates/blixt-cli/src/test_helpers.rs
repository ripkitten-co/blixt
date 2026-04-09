#![cfg(test)]

use std::path::PathBuf;

use tempfile::TempDir;

pub struct TestProject {
    pub dir: TempDir,
}

impl TestProject {
    pub fn new() -> Self {
        Self {
            dir: TempDir::new().expect("failed to create temp dir"),
        }
    }

    pub fn path(&self) -> &std::path::Path {
        self.dir.path()
    }

    pub fn persist_on_failure(self) -> PathBuf {
        let path = self.dir.keep();
        eprintln!("test project preserved at: {}", path.display());
        path
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
