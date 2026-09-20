use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn export_creates_safety_gitignore_in_requested_backup_folder() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let backup_dir = std::env::temp_dir().join(format!("aiws-export-test-{unique}"));

    let status = Command::new(env!("CARGO_BIN_EXE_aiws"))
        .args(["export", "--repo", backup_dir.to_str().unwrap()])
        .status()
        .unwrap();

    assert!(status.success());
    let gitignore = fs::read_to_string(backup_dir.join(".gitignore")).unwrap();
    assert!(gitignore.contains(".env"));
    assert!(gitignore.contains("*token*"));

    fs::remove_dir_all(backup_dir).unwrap();
}

#[test]
fn export_to_git_url_commits_and_pushes_backup() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("aiws-git-export-test-{unique}"));
    let remote = root.join("remote.git");
    let backup_dir = root.join("backup");
    fs::create_dir_all(&root).unwrap();

    assert!(Command::new("git")
        .args(["init", "--bare", remote.to_str().unwrap()])
        .status()
        .unwrap()
        .success());

    let remote_url = format!("file:///{}", remote.to_string_lossy().replace('\\', "/"));
    let status = Command::new(env!("CARGO_BIN_EXE_aiws"))
        .args([
            "export",
            "--repo",
            &remote_url,
            "--dir",
            backup_dir.to_str().unwrap(),
        ])
        .status()
        .unwrap();

    assert!(status.success());
    assert!(Command::new("git")
        .args([
            "--git-dir",
            remote.to_str().unwrap(),
            "show-ref",
            "--verify",
            "--quiet",
            "refs/heads/main",
        ])
        .status()
        .unwrap()
        .success());

    fs::remove_dir_all(root).unwrap();
}
