use std::fs;
use std::path::{Path, PathBuf};

/// Returns the current working directory as a `PathBuf`.
pub fn current_dir() -> Result<PathBuf, String> {
    std::env::current_dir().map_err(|err| format!("Failed to determine current directory: {err}"))
}

/// Creates a directory and all parents, returning an error on failure.
pub fn ensure_dir_exists(dir: &Path) -> Result<(), String> {
    fs::create_dir_all(dir)
        .map_err(|err| format!("Failed to create directory '{}': {err}", dir.display()))
}

/// Writes content to a file, failing if the file already exists.
pub fn write_file(path: &Path, content: &str) -> Result<(), String> {
    if path.exists() {
        return Err(format!("File already exists: {}", path.display()));
    }
    fs::write(path, content).map_err(|err| format!("Failed to write '{}': {err}", path.display()))
}

/// Appends a `pub mod {name};` line to a module's `mod.rs`, creating the file if needed.
pub fn update_mod_file(base: &Path, module_dir: &str, name: &str) -> Result<(), String> {
    let mod_path = base.join(format!("src/{module_dir}/mod.rs"));
    let mod_line = format!("pub mod {name};");

    if mod_path.exists() {
        let content = fs::read_to_string(&mod_path)
            .map_err(|e| format!("Failed to read {}: {e}", mod_path.display()))?;
        if content.contains(&mod_line) {
            return Ok(());
        }
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(&mod_path)
            .map_err(|e| format!("Failed to open {}: {e}", mod_path.display()))?;
        use std::io::Write;
        writeln!(file, "{mod_line}")
            .map_err(|e| format!("Failed to write {}: {e}", mod_path.display()))?;
    } else {
        ensure_dir_exists(&base.join(format!("src/{module_dir}")))?;
        fs::write(&mod_path, format!("{mod_line}\n"))
            .map_err(|e| format!("Failed to write {}: {e}", mod_path.display()))?;
    }
    Ok(())
}
