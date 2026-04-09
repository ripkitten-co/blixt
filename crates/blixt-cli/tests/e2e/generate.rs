use std::fs;
use std::process::Command;

use tempfile::TempDir;

fn setup_project_dir() -> TempDir {
    let tmp = TempDir::new().expect("temp dir");
    let base = tmp.path();

    fs::create_dir_all(base.join("src/controllers")).expect("create controllers dir");
    fs::create_dir_all(base.join("templates/pages")).expect("create templates dir");
    fs::create_dir_all(base.join("templates/fragments")).expect("create fragments dir");
    fs::create_dir_all(base.join("migrations")).expect("create migrations dir");
    fs::create_dir_all(base.join("src/models")).expect("create models dir");
    fs::write(base.join("Cargo.toml"), "[package]\nname = \"test\"\n").expect("write Cargo.toml");

    tmp
}

fn blixt_binary() -> String {
    let mut path = std::env::current_exe()
        .expect("current exe")
        .parent()
        .expect("parent dir")
        .parent()
        .expect("target dir")
        .to_path_buf();
    path.push("blixt");
    if cfg!(windows) {
        path.set_extension("exe");
    }
    path.to_string_lossy().to_string()
}

#[test]
#[ignore]
fn generate_controller_creates_files() {
    let project = setup_project_dir();

    let output = Command::new(blixt_binary())
        .args(["generate", "controller", "blog_post"])
        .current_dir(project.path())
        .output()
        .expect("failed to run blixt generate controller");

    assert!(
        output.status.success(),
        "generate controller should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(project.path().join("src/controllers/blog_post.rs").exists());
    assert!(
        project
            .path()
            .join("templates/pages/blog_post/index.html")
            .exists()
    );
    assert!(
        project
            .path()
            .join("templates/pages/blog_post/show.html")
            .exists()
    );
}

#[test]
#[ignore]
fn generate_model_creates_files() {
    let project = setup_project_dir();

    let output = Command::new(blixt_binary())
        .args(["generate", "model", "user"])
        .current_dir(project.path())
        .output()
        .expect("failed to run blixt generate model");

    assert!(
        output.status.success(),
        "generate model should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(project.path().join("src/models/user.rs").exists());

    let migrations: Vec<_> = fs::read_dir(project.path().join("migrations"))
        .expect("read migrations")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "sql"))
        .collect();
    assert_eq!(migrations.len(), 1, "should create exactly one migration");
}

#[test]
#[ignore]
fn generate_scaffold_creates_all_files() {
    let project = setup_project_dir();

    let output = Command::new(blixt_binary())
        .args(["generate", "scaffold", "product"])
        .current_dir(project.path())
        .output()
        .expect("failed to run blixt generate scaffold");

    assert!(
        output.status.success(),
        "generate scaffold should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(project.path().join("src/controllers/product.rs").exists());
    assert!(project.path().join("src/models/product.rs").exists());
    assert!(
        project
            .path()
            .join("templates/pages/product/index.html")
            .exists()
    );
    assert!(
        project
            .path()
            .join("templates/pages/product/show.html")
            .exists()
    );
    assert!(
        project
            .path()
            .join("templates/fragments/product/list.html")
            .exists()
    );
}
