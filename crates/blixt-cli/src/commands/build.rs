use std::path::Path;
use std::process::Command;

use console::style;

use crate::tailwind;

/// Builds the Blixt project for production deployment.
///
/// Steps: verify project, compile Tailwind CSS (minified), run `cargo build --release`,
/// then report the output binary path and size.
pub async fn run() -> Result<(), String> {
    verify_blixt_project()?;
    println!("  {} Building for production...", style("▸").cyan().bold());

    build_tailwind().await?;
    build_cargo()?;
    report_binary_size()?;

    println!("  {} Production build complete.", style("✓").green().bold());
    Ok(())
}

/// Verifies that we are inside a Blixt project directory.
fn verify_blixt_project() -> Result<(), String> {
    if !Path::new("Cargo.toml").exists() {
        return Err(
            "No Cargo.toml found. Run this command from a Blixt project directory.".to_string(),
        );
    }
    Ok(())
}

/// Downloads Tailwind (if needed) and compiles CSS in production mode.
async fn build_tailwind() -> Result<(), String> {
    println!(
        "  {} Compiling Tailwind CSS (minified)...",
        style("▸").cyan().bold()
    );

    let tailwind_binary = tailwind::ensure_tailwind().await?;
    let status = Command::new(&tailwind_binary)
        .args([
            "--input",
            "static/css/app.css",
            "--output",
            "static/css/output.css",
            "--minify",
        ])
        .status()
        .map_err(|err| format!("Failed to run Tailwind CSS: {err}"))?;

    if !status.success() {
        return Err(format!("Tailwind CSS exited with: {status}"));
    }
    Ok(())
}

/// Runs `cargo build --release` and checks for success.
fn build_cargo() -> Result<(), String> {
    println!(
        "  {} Compiling Rust (release mode)...",
        style("▸").cyan().bold()
    );

    let status = Command::new("cargo")
        .args(["build", "--release"])
        .status()
        .map_err(|err| format!("Failed to run cargo build: {err}"))?;

    if !status.success() {
        return Err(format!("cargo build --release exited with: {status}"));
    }
    Ok(())
}

/// Finds the release binary and prints its path and approximate size.
fn report_binary_size() -> Result<(), String> {
    let project_name = read_project_name()?;
    let binary_path = Path::new("target/release").join(&project_name);

    if !binary_path.exists() {
        println!(
            "  {} Binary not found at expected path: {}",
            style("!").yellow().bold(),
            binary_path.display()
        );
        return Ok(());
    }

    let metadata = std::fs::metadata(&binary_path)
        .map_err(|err| format!("Failed to read binary metadata: {err}"))?;

    let size_mb = metadata.len() as f64 / (1024.0 * 1024.0);
    println!(
        "  {} Binary: {} ({size_mb:.1} MB)",
        style("▸").cyan().bold(),
        binary_path.display()
    );

    Ok(())
}

/// Reads the project name from Cargo.toml `[package] name = "..."`.
fn read_project_name() -> Result<String, String> {
    let contents = std::fs::read_to_string("Cargo.toml")
        .map_err(|err| format!("Failed to read Cargo.toml: {err}"))?;

    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("name")
            && trimmed.contains('=')
            && let Some(value) = trimmed.split('=').nth(1)
        {
            let name = value.trim().trim_matches('"').trim_matches('\'');
            if !name.is_empty() {
                return Ok(name.to_string());
            }
        }
    }

    Err("Could not find package name in Cargo.toml".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static CWD_LOCK: Mutex<()> = Mutex::new(());

    #[tokio::test]
    async fn build_fails_gracefully_outside_project() {
        let _guard = CWD_LOCK.lock().expect("lock");

        let temp_dir = std::env::temp_dir().join("blixt-build-test-no-project");
        let _ = std::fs::create_dir_all(&temp_dir);

        let original_dir = std::env::current_dir().expect("get cwd");
        std::env::set_current_dir(&temp_dir).expect("cd to temp");

        // Ensure no Cargo.toml exists
        let _ = std::fs::remove_file(temp_dir.join("Cargo.toml"));

        let result = run().await;

        std::env::set_current_dir(&original_dir).expect("restore cwd");
        let _ = std::fs::remove_dir_all(&temp_dir);

        assert!(result.is_err());
        let err_msg = result.expect_err("should fail");
        assert!(
            err_msg.contains("No Cargo.toml found"),
            "Unexpected error: {err_msg}"
        );
    }

    #[test]
    fn read_project_name_parses_standard_cargo_toml() {
        let _guard = CWD_LOCK.lock().expect("lock");

        let tmp = tempfile::TempDir::new().expect("temp dir");
        let cargo_toml = tmp.path().join("Cargo.toml");
        std::fs::write(&cargo_toml, "[package]\nname = \"my-app\"\nversion = \"0.1.0\"\n")
            .expect("write");

        let original = std::env::current_dir().expect("cwd");
        std::env::set_current_dir(tmp.path()).expect("cd");

        let result = read_project_name();

        std::env::set_current_dir(original).expect("restore cwd");
        assert_eq!(result.unwrap(), "my-app");
    }

    #[test]
    fn read_project_name_fails_without_name_field() {
        let _guard = CWD_LOCK.lock().expect("lock");

        let tmp = tempfile::TempDir::new().expect("temp dir");
        let cargo_toml = tmp.path().join("Cargo.toml");
        std::fs::write(&cargo_toml, "[package]\nversion = \"0.1.0\"\n")
            .expect("write");

        let original = std::env::current_dir().expect("cwd");
        std::env::set_current_dir(tmp.path()).expect("cd");

        let result = read_project_name();

        std::env::set_current_dir(original).expect("restore cwd");
        assert!(result.is_err());
    }
}
