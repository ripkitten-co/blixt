use std::path::Path;
use std::time::Duration;

use console::style;
use notify::{EventKind, RecursiveMode, Watcher};
use tokio::process::Command;
use tokio::sync::mpsc;

use crate::tailwind;

/// Starts the development server with file watching for auto-restart.
pub async fn run() -> Result<(), String> {
    verify_blixt_project()?;
    println!(
        "  {} Starting Blixt dev server...",
        style("▸").cyan().bold()
    );

    let tailwind_path = tailwind::ensure_tailwind().await?;
    let mut tailwind_child = spawn_tailwind(&tailwind_path)?;

    let (file_tx, mut file_rx) = mpsc::channel::<()>(1);
    let _watcher = start_file_watcher(file_tx)?;

    let mut cargo_child = spawn_cargo()?;

    loop {
        tokio::select! {
            result = cargo_child.wait() => {
                match result {
                    Ok(status) if status.success() => break,
                    Ok(status) => {
                        println!("  {} App exited ({}), waiting for file changes...",
                            style("!").yellow().bold(), status);
                        wait_for_change(&mut file_rx).await;
                        cargo_child = spawn_cargo()?;
                    }
                    Err(e) => return Err(format!("cargo run error: {e}")),
                }
            }
            _ = file_rx.recv() => {
                println!("  {} File changed, restarting...",
                    style("↻").cyan().bold());
                kill_child(&mut cargo_child, "app");
                let _ = cargo_child.wait().await;
                debounce(&mut file_rx).await;
                cargo_child = spawn_cargo()?;
            }
            _ = tokio::signal::ctrl_c() => {
                println!("\n  {} Shutting down...", style("▸").yellow().bold());
                break;
            }
        }
    }

    kill_child(&mut tailwind_child, "Tailwind");
    kill_child(&mut cargo_child, "cargo");
    Ok(())
}

fn verify_blixt_project() -> Result<(), String> {
    if !Path::new("Cargo.toml").exists() {
        return Err("No Cargo.toml found. Run this command from a Blixt project directory.".into());
    }
    Ok(())
}

fn spawn_tailwind(binary: &Path) -> Result<tokio::process::Child, String> {
    println!(
        "  {} Tailwind CSS watching for changes...",
        style("▸").cyan().bold()
    );
    Command::new(binary)
        .args([
            "--input",
            "static/css/app.css",
            "--output",
            "static/css/output.css",
            "--watch",
        ])
        .kill_on_drop(true)
        .spawn()
        .map_err(|err| format!("Failed to start Tailwind watcher: {err}"))
}

fn spawn_cargo() -> Result<tokio::process::Child, String> {
    println!(
        "  {} Compiling and starting application...",
        style("▸").cyan().bold()
    );
    Command::new("cargo")
        .arg("run")
        .kill_on_drop(true)
        .spawn()
        .map_err(|err| format!("Failed to start cargo run: {err}"))
}

fn start_file_watcher(tx: mpsc::Sender<()>) -> Result<notify::RecommendedWatcher, String> {
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        let Ok(event) = event else { return };
        if !is_relevant_change(&event) {
            return;
        }
        let _ = tx.try_send(());
    })
    .map_err(|e| format!("Failed to create file watcher: {e}"))?;

    for dir in ["src", "templates"] {
        if Path::new(dir).exists() {
            watcher
                .watch(Path::new(dir), RecursiveMode::Recursive)
                .map_err(|e| format!("Failed to watch {dir}: {e}"))?;
        }
    }

    println!(
        "  {} Watching src/ and templates/ for changes...",
        style("▸").cyan().bold()
    );
    Ok(watcher)
}

fn is_relevant_change(event: &notify::Event) -> bool {
    if !matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_)) {
        return false;
    }
    event.paths.iter().any(|p| {
        let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
        matches!(ext, "rs" | "html" | "toml")
    })
}

async fn wait_for_change(rx: &mut mpsc::Receiver<()>) {
    rx.recv().await;
}

async fn debounce(rx: &mut mpsc::Receiver<()>) {
    tokio::time::sleep(Duration::from_millis(300)).await;
    while rx.try_recv().is_ok() {}
}

fn kill_child(child: &mut tokio::process::Child, label: &str) {
    if let Err(err) = child.start_kill() {
        eprintln!(
            "  {} Failed to stop {label}: {err}",
            style("!").yellow().bold()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn modify_event(path: &str) -> notify::Event {
        notify::Event {
            kind: EventKind::Modify(notify::event::ModifyKind::Data(
                notify::event::DataChange::Content,
            )),
            paths: vec![PathBuf::from(path)],
            attrs: Default::default(),
        }
    }

    #[test]
    fn relevant_change_accepts_rust_files() {
        assert!(is_relevant_change(&modify_event("src/main.rs")));
    }

    #[test]
    fn relevant_change_accepts_html_files() {
        assert!(is_relevant_change(&modify_event("templates/home.html")));
    }

    #[test]
    fn relevant_change_accepts_toml_files() {
        assert!(is_relevant_change(&modify_event("Cargo.toml")));
    }

    #[test]
    fn relevant_change_ignores_css_files() {
        assert!(!is_relevant_change(&modify_event("static/css/output.css")));
    }

    #[test]
    fn relevant_change_ignores_delete_events() {
        let event = notify::Event {
            kind: EventKind::Remove(notify::event::RemoveKind::File),
            paths: vec![PathBuf::from("src/main.rs")],
            attrs: Default::default(),
        };
        assert!(!is_relevant_change(&event));
    }

    #[test]
    fn relevant_change_accepts_create_events() {
        let event = notify::Event {
            kind: EventKind::Create(notify::event::CreateKind::File),
            paths: vec![PathBuf::from("src/new_file.rs")],
            attrs: Default::default(),
        };
        assert!(is_relevant_change(&event));
    }
}
