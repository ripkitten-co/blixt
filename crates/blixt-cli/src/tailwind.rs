use std::fs;
use std::path::{Path, PathBuf};

use indicatif::{ProgressBar, ProgressStyle};
use sha2::{Digest, Sha256};

const TAILWIND_VERSION: &str = "4.1.8";

/// Hardcoded download base URL. Must ONLY point to the official GitHub releases.
/// Do not accept download URLs from configuration or environment variables.
const DOWNLOAD_BASE: &str = "https://github.com/tailwindlabs/tailwindcss/releases/download";

// SHA-256 checksums from https://github.com/tailwindlabs/tailwindcss/releases/tag/v4.1.8
const TAILWIND_CHECKSUMS: &[(&str, &str)] = &[
    (
        "macos-arm64",
        "19e52791d356dd59db68274ae36a5879bab0ce9dac23cc7b0f19fc7b7c1d37a2",
    ),
    (
        "macos-x64",
        "4a6cb260d75c4bdca0724fbcc3b23a5adb52715ad6d78595463c86128ca1c329",
    ),
    (
        "linux-arm64",
        "28a77d1e59b0e45b41683c1e3947621fdfe73f6895b05db7c34f63f3f4898e8d",
    ),
    (
        "linux-x64",
        "8f84ce810bdff225e599781d1e2daa82b4282229021c867a71b419f59f9aa836",
    ),
];

/// Detects the current platform and returns the Tailwind platform suffix.
fn detect_platform() -> Result<&'static str, String> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Ok("macos-arm64"),
        ("macos", "x86_64") => Ok("macos-x64"),
        ("linux", "aarch64") => Ok("linux-arm64"),
        ("linux", "x86_64") => Ok("linux-x64"),
        (os, arch) => Err(format!("Unsupported platform: {os}-{arch}")),
    }
}

/// Returns the expected SHA-256 checksum for a given platform.
fn expected_checksum(platform: &str) -> Result<&'static str, String> {
    TAILWIND_CHECKSUMS
        .iter()
        .find(|(p, _)| *p == platform)
        .map(|(_, hash)| *hash)
        .ok_or_else(|| format!("No checksum defined for platform: {platform}"))
}

/// Builds the cache directory path: `~/.cache/blixt/`.
fn cache_dir() -> Result<PathBuf, String> {
    let base = dirs::cache_dir()
        .ok_or_else(|| "Could not determine system cache directory".to_string())?;
    Ok(base.join("blixt"))
}

/// Builds the full cache path for a specific version and platform.
fn cached_binary_path(platform: &str) -> Result<PathBuf, String> {
    let dir = cache_dir()?;
    let filename = format!("tailwindcss-v{TAILWIND_VERSION}-{platform}");
    Ok(dir.join(filename))
}

/// Builds the download URL for a specific platform.
fn download_url(platform: &str) -> String {
    format!("{DOWNLOAD_BASE}/v{TAILWIND_VERSION}/tailwindcss-{platform}")
}

/// Verifies that the SHA-256 checksum of a file matches the expected hex digest.
///
/// Returns `Ok(true)` if the checksums match, `Ok(false)` if they differ,
/// or an error if the file cannot be read.
pub fn verify_checksum(path: &Path, expected_hex: &str) -> Result<bool, String> {
    let contents =
        fs::read(path).map_err(|err| format!("Failed to read file for checksum: {err}"))?;
    let digest = Sha256::digest(&contents);
    let actual_hex = format!("{digest:x}");
    Ok(actual_hex == expected_hex.to_lowercase())
}

/// Ensures the Tailwind CSS binary is available, downloading and verifying it
/// if necessary.
///
/// Returns the path to the verified, executable binary. On cache hit the
/// existing binary is re-verified against its pinned SHA-256 checksum; on
/// mismatch it is deleted and re-downloaded.
pub async fn ensure_tailwind() -> Result<PathBuf, String> {
    let platform = detect_platform()?;
    let expected = expected_checksum(platform)?;
    let path = cached_binary_path(platform)?;

    if path.exists() {
        if verify_checksum(&path, expected)? {
            return Ok(path);
        }
        remove_file_safe(&path)?;
    }

    download_binary(platform, &path).await?;
    verify_after_download(&path, expected)?;
    set_executable(&path)?;

    Ok(path)
}

/// Removes a file, mapping I/O errors to a descriptive string.
fn remove_file_safe(path: &Path) -> Result<(), String> {
    fs::remove_file(path).map_err(|err| format!("Failed to remove corrupted cached binary: {err}"))
}

/// Downloads the Tailwind binary for the given platform to `dest`.
async fn download_binary(platform: &str, dest: &Path) -> Result<(), String> {
    let url = download_url(platform);
    ensure_parent_dir(dest)?;

    let response = build_client()?
        .get(&url)
        .send()
        .await
        .map_err(|err| format!("Download request failed: {err}"))?;

    let status = response.status();
    if !status.is_success() {
        return Err(format!("Download failed with HTTP {status}: {url}"));
    }

    let total_size = response.content_length();
    let progress_bar = create_progress_bar(total_size);

    stream_to_file(response, dest, &progress_bar).await?;
    progress_bar.finish_with_message("Download complete");

    Ok(())
}

/// Creates the parent directory for a path if it does not exist.
fn ensure_parent_dir(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("Failed to create cache directory: {err}"))?;
    }
    Ok(())
}

/// Builds a reqwest client with appropriate timeouts and redirect policy.
fn build_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(30))
        .timeout(std::time::Duration::from_secs(300))
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .map_err(|err| format!("Failed to build HTTP client: {err}"))
}

/// Creates a progress bar for the download.
fn create_progress_bar(total_size: Option<u64>) -> ProgressBar {
    let progress_bar = match total_size {
        Some(size) => ProgressBar::new(size),
        None => ProgressBar::new_spinner(),
    };

    let template = "{spinner:.green} [{elapsed_precise}] \
                    [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})";

    if let Ok(style) = ProgressStyle::default_bar().template(template) {
        progress_bar.set_style(style.progress_chars("#>-"));
    }

    progress_bar
}

/// Writes an HTTP response body to a file while updating a progress bar.
async fn stream_to_file(
    response: reqwest::Response,
    dest: &Path,
    progress_bar: &ProgressBar,
) -> Result<(), String> {
    use std::io::Write;

    let bytes = response
        .bytes()
        .await
        .map_err(|err| format!("Failed to read response body: {err}"))?;

    progress_bar.set_position(bytes.len() as u64);

    let mut file = fs::File::create(dest)
        .map_err(|err| format!("Failed to create file {}: {err}", dest.display()))?;

    file.write_all(&bytes)
        .map_err(|err| format!("Failed to write binary to disk: {err}"))?;

    Ok(())
}

/// Verifies the downloaded file and returns an error with details on mismatch.
fn verify_after_download(path: &Path, expected: &str) -> Result<(), String> {
    if verify_checksum(path, expected)? {
        return Ok(());
    }

    let contents =
        fs::read(path).map_err(|err| format!("Failed to read downloaded file: {err}"))?;
    let actual = format!("{:x}", Sha256::digest(&contents));

    let _ = fs::remove_file(path);

    Err(format!(
        "Checksum verification failed for Tailwind CSS binary.\n\
         Expected: {expected}\n\
         Actual:   {actual}\n\
         The downloaded file has been removed."
    ))
}

/// Sets the executable permission bit on Unix systems.
#[cfg(unix)]
fn set_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path)
        .map_err(|err| format!("Failed to read file metadata: {err}"))?
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms)
        .map_err(|err| format!("Failed to set executable permission: {err}"))
}

/// No-op on non-Unix platforms.
#[cfg(not(unix))]
fn set_executable(_path: &Path) -> Result<(), String> {
    Ok(())
}

/// Spawns the Tailwind CSS binary as a child process with the given arguments.
///
/// Returns the `Child` handle so the caller can manage the process lifetime
/// (e.g., for watch mode).
#[allow(dead_code)]
pub fn run_tailwind(binary: &Path, args: &[&str]) -> Result<std::process::Child, String> {
    std::process::Command::new(binary)
        .args(args)
        .spawn()
        .map_err(|err| format!("Failed to spawn Tailwind process: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn detect_platform_returns_valid_string() {
        let result = detect_platform();
        // On macOS or Linux x86_64/aarch64, this should succeed.
        // On other platforms, it should return a descriptive error.
        match result {
            Ok(platform) => {
                let valid = ["macos-arm64", "macos-x64", "linux-arm64", "linux-x64"];
                assert!(valid.contains(&platform), "Unexpected platform: {platform}");
            }
            Err(msg) => {
                assert!(msg.starts_with("Unsupported platform:"));
            }
        }
    }

    #[test]
    fn verify_checksum_matches_known_digest() {
        let dir = std::env::temp_dir().join("blixt-test-checksum-match");
        let _ = fs::create_dir_all(&dir);
        let file_path = dir.join("known.bin");

        let mut file = fs::File::create(&file_path).expect("create test file");
        file.write_all(b"hello world").expect("write test data");

        // SHA-256 of "hello world"
        let expected = "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";

        let result = verify_checksum(&file_path, expected);
        assert!(result.is_ok());
        assert!(result.expect("should not error") == true);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_checksum_rejects_wrong_digest() {
        let dir = std::env::temp_dir().join("blixt-test-checksum-mismatch");
        let _ = fs::create_dir_all(&dir);
        let file_path = dir.join("wrong.bin");

        let mut file = fs::File::create(&file_path).expect("create test file");
        file.write_all(b"hello world").expect("write test data");

        let wrong_hash = "0000000000000000000000000000000000000000000000000000000000000000";

        let result = verify_checksum(&file_path, wrong_hash);
        assert!(result.is_ok());
        assert!(result.expect("should not error") == false);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_checksum_case_insensitive() {
        let dir = std::env::temp_dir().join("blixt-test-checksum-case");
        let _ = fs::create_dir_all(&dir);
        let file_path = dir.join("case.bin");

        let mut file = fs::File::create(&file_path).expect("create test file");
        file.write_all(b"hello world").expect("write test data");

        let expected_upper = "B94D27B9934D3E08A52E52D7DA7DABFAC484EFE37A5380EE9088F7ACE2EFCDE9";

        let result = verify_checksum(&file_path, expected_upper);
        assert!(result.is_ok());
        assert!(result.expect("should not error") == true);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn cache_path_construction_is_correct() {
        let path = cached_binary_path("macos-arm64");
        assert!(path.is_ok());

        let path = path.expect("should produce a path");
        let filename = path
            .file_name()
            .expect("should have filename")
            .to_string_lossy();

        assert_eq!(
            filename,
            format!("tailwindcss-v{TAILWIND_VERSION}-macos-arm64")
        );

        let parent = path.parent().expect("should have parent");
        assert!(parent.ends_with("blixt"));
    }

    #[test]
    fn download_url_uses_hardcoded_base() {
        let url = download_url("macos-arm64");
        assert!(url.starts_with(DOWNLOAD_BASE));
        assert!(url.contains(&format!("v{TAILWIND_VERSION}")));
        assert!(url.ends_with("tailwindcss-macos-arm64"));
    }

    #[test]
    fn download_url_contains_no_external_input() {
        let url = download_url("linux-x64");
        let expected = format!("{DOWNLOAD_BASE}/v{TAILWIND_VERSION}/tailwindcss-linux-x64");
        assert_eq!(url, expected);
    }

    #[test]
    fn expected_checksum_returns_value_for_known_platforms() {
        for (platform, _) in TAILWIND_CHECKSUMS {
            let result = expected_checksum(platform);
            assert!(result.is_ok(), "Missing checksum for {platform}");
        }
    }

    #[test]
    fn expected_checksum_errors_for_unknown_platform() {
        let result = expected_checksum("windows-x64");
        assert!(result.is_err());
        assert!(
            result
                .expect_err("should error")
                .contains("No checksum defined")
        );
    }
}
