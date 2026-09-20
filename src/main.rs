use clap::{Parser, Subcommand};
use directories::BaseDirs;
use regex::Regex;
use std::error::Error;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, Stdio};

const GITIGNORE_CONTENT: &str = "\
# Never commit secrets or local runtime data
.env
*.key
*.pem
*token*
*credential*
*auth*
history.jsonl
cache/
sessions/
";

#[derive(Parser, Debug)]
#[command(name = "aiws", version, about = "Back up portable AI coding-tool settings")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Copy safe local settings into the backup folder.
    Export {
        /// Backup folder or Git repository URL. Default: %USERPROFILE%\.aiws-backup
        #[arg(long)]
        repo: Option<String>,

        /// Local folder used when --repo is a Git URL.
        #[arg(long)]
        dir: Option<PathBuf>,
    },

    /// Restore saved settings from the backup folder.
    Import {
        /// Optional backup folder. Default: %USERPROFILE%\.aiws-backup
        #[arg(long)]
        repo: Option<String>,

        /// Overwrite an existing local settings file.
        #[arg(long)]
        force: bool,
    },
}

struct Setting {
    tool: &'static str,
    source: PathBuf,
    backup_path: &'static str,
}

#[cfg(test)]
mod tests {
    use super::sanitize_settings_content;

    #[test]
    fn sanitizes_common_secret_values() {
        let content = r#"api_key = "sk-test-secret"
token: ghp_abcdefghijklmnopqrstuvwx
Authorization: Bearer eyJ.example.signature
unstructured_value = sk-abcdefghijklmnopqrstuvwx
"#;

        let sanitized = sanitize_settings_content(content);

        assert!(!sanitized.contains("sk-test-secret"));
        assert!(!sanitized.contains("ghp_abcdefghijklmnopqrstuvwx"));
        assert!(!sanitized.contains("eyJ.example.signature"));
        assert!(!sanitized.contains("sk-abcdefghijklmnopqrstuvwx"));
        assert_eq!(sanitized.matches("****").count(), 4);
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();
    let settings = supported_settings()?;

    match cli.command {
        Command::Export { repo, dir } => {
            let backup_dir = export_backup_dir(repo.as_deref(), dir)?;
            let remote_url = repo.as_deref().filter(|repo| is_git_url(repo));

            if let Some(remote_url) = remote_url {
                prepare_git_repository(&backup_dir, remote_url)?;
            }

            export_settings(&backup_dir, &settings)?;

            if remote_url.is_some() {
                commit_and_push(&backup_dir)?;
            }
        }

        Command::Import { repo, force } => {
            let backup_dir = repo.map(PathBuf::from).unwrap_or(default_backup_dir()?);
            import_settings(&backup_dir, &settings, force)?;
        }
    }

    Ok(())
}

fn export_backup_dir(
    repo: Option<&str>,
    dir: Option<PathBuf>,
) -> Result<PathBuf, Box<dyn Error>> {
    if let Some(dir) = dir {
        return Ok(dir);
    }

    match repo {
        Some(repo) if !is_git_url(repo) => Ok(PathBuf::from(repo)),
        _ => default_backup_dir(),
    }
}

fn is_git_url(value: &str) -> bool {
    value.starts_with("https://")
        || value.starts_with("http://")
        || value.starts_with("ssh://")
        || value.starts_with("git@")
        || value.starts_with("file://")
}

fn prepare_git_repository(backup_dir: &Path, remote_url: &str) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(backup_dir)?;
    run_git(backup_dir, ["init"])?;

    if !git_succeeds(backup_dir, ["remote", "get-url", "origin"])? {
        run_git(backup_dir, ["remote", "add", "origin", remote_url])?;
    } else {
        run_git(backup_dir, ["remote", "set-url", "origin", remote_url])?;
    }

    if run_git(backup_dir, ["config", "user.name"]).is_err() {
        run_git(backup_dir, ["config", "user.name", "aiws"])?;
    }

    if run_git(backup_dir, ["config", "user.email"]).is_err() {
        run_git(backup_dir, ["config", "user.email", "aiws@localhost"])?;
    }

    Ok(())
}

fn commit_and_push(backup_dir: &Path) -> Result<(), Box<dyn Error>> {
    run_git(backup_dir, ["add", "."])?;

    if run_git(backup_dir, ["diff", "--cached", "--quiet"]).is_ok() {
        println!("Git backup is already up to date.");
        return Ok(());
    }

    run_git(backup_dir, ["commit", "-m", "aiws: update settings backup"])?;
    run_git(backup_dir, ["branch", "-M", "main"])?;
    run_git(backup_dir, ["push", "-u", "origin", "main"])?;
    println!("Pushed backup to GitHub.");

    Ok(())
}

fn git_succeeds<const N: usize>(
    backup_dir: &Path,
    args: [&str; N],
) -> Result<bool, Box<dyn Error>> {
    Ok(ProcessCommand::new("git")
        .args(args)
        .current_dir(backup_dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?
        .success())
}

fn run_git<const N: usize>(backup_dir: &Path, args: [&str; N]) -> Result<(), Box<dyn Error>> {
    let status = ProcessCommand::new("git")
        .args(args)
        .current_dir(backup_dir)
        .status()?;

    if status.success() {
        Ok(())
    } else {
        Err(Box::new(io::Error::new(
            io::ErrorKind::Other,
            "Git command failed",
        )))
    }
}

fn default_backup_dir() -> Result<PathBuf, Box<dyn Error>> {
    let base_dirs = BaseDirs::new().ok_or_else(|| {
        io::Error::new(io::ErrorKind::NotFound, "Could not find your home folder")
    })?;

    Ok(base_dirs.home_dir().join(".aiws-backup"))
}

fn sanitize_settings_content(content: &str) -> String {
    let known_api_key = Regex::new(
        r"(?:sk-(?:proj-|ant-)?[A-Za-z0-9_-]{10,}|AIza[A-Za-z0-9_-]{20,}|ghp_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,})",
    )
    .expect("known API key regex is valid");
    let authorization = Regex::new(
        r#"(?im)(?P<prefix>[\"']?authorization[\"']?\s*(?:=|:)\s*[\"']?)(?:Bearer\s+)?(?P<value>[^\s,\"'}\]]+)"#,
    )
    .expect("authorization regex is valid");
    let key_value = Regex::new(
        r#"(?im)(?P<prefix>[\"']?(?:api[_-]?key|access[_-]?token|refresh[_-]?token|token|secret|password|credential)[\"']?\s*(?:=|:)\s*[\"']?)(?P<value>[^\s,\"'}\]]+)"#,
    )
    .expect("secret key/value regex is valid");
    let bearer = Regex::new(r"(?i)(?P<prefix>\bBearer\s+)(?P<value>[A-Za-z0-9._~-]+)")
        .expect("bearer token regex is valid");
    let query_value = Regex::new(
        r"(?i)(?P<prefix>[?&](?:api[_-]?key|access[_-]?token|token|secret)=)(?P<value>[^&\s]+)",
    )
    .expect("secret query regex is valid");

    let sanitized = known_api_key.replace_all(content, "****");
    let sanitized = authorization.replace_all(&sanitized, "${prefix}****");
    let sanitized = key_value.replace_all(&sanitized, "${prefix}****");
    let sanitized = bearer.replace_all(&sanitized, "${prefix}****");
    query_value.replace_all(&sanitized, "${prefix}****").into_owned()
}

fn copy_sanitized(source: &Path, destination: &Path) -> Result<(), Box<dyn Error>> {
    let content = fs::read_to_string(source)?;
    fs::write(destination, sanitize_settings_content(&content))?;
    Ok(())
}

fn supported_settings() -> Result<Vec<Setting>, Box<dyn Error>> {
    let base_dirs = BaseDirs::new().ok_or_else(|| {
        io::Error::new(io::ErrorKind::NotFound, "Could not find your home folder")
    })?;

    let home = base_dirs.home_dir();
    let config = base_dirs.config_dir();

    let vscode_user_folder = if cfg!(target_os = "macos") {
        home.join("Library/Application Support/Code/User")
    } else {
        config.join("Code/User")
    };

    Ok(vec![
        Setting {
            tool: "Claude Code settings",
            source: home.join(".claude/settings.json"),
            backup_path: "claude/settings.json",
        },
        Setting {
            tool: "Claude Code instructions",
            source: home.join(".claude/CLAUDE.md"),
            backup_path: "claude/CLAUDE.md",
        },
        Setting {
            tool: "Codex CLI settings",
            source: home.join(".codex/config.toml"),
            backup_path: "codex/config.toml",
        },
        Setting {
            tool: "VS Code / Copilot settings",
            source: vscode_user_folder.join("settings.json"),
            backup_path: "vscode/settings.json",
        },
        Setting {
            tool: "VS Code keybindings",
            source: vscode_user_folder.join("keybindings.json"),
            backup_path: "vscode/keybindings.json",
        },
    ])
}

fn export_settings(backup_dir: &Path, settings: &[Setting]) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(backup_dir)?;

    let gitignore_path = backup_dir.join(".gitignore");

    if !gitignore_path.exists() {
        fs::write(&gitignore_path, GITIGNORE_CONTENT)?;
        println!("Created {}", gitignore_path.display());
    }

    println!("\nExporting settings to {}\n", backup_dir.display());

    let mut copied = 0;

    for setting in settings {
        if !setting.source.is_file() {
            println!("SKIP  {} — file not found", setting.tool);
            continue;
        }

        let destination = backup_dir.join(setting.backup_path);
        let parent = destination.parent().ok_or_else(|| {
            io::Error::new(io::ErrorKind::Other, "Backup destination has no parent folder")
        })?;

        fs::create_dir_all(parent)?;
        copy_sanitized(&setting.source, &destination)?;

        println!("COPY  {} → {}", setting.tool, destination.display());
        copied += 1;
    }

    println!("\nExport complete: {copied} file(s) copied.");
    println!("Review the files before committing them to a private Git repository.");

    Ok(())
}

fn import_settings(
    backup_dir: &Path,
    settings: &[Setting],
    force: bool,
) -> Result<(), Box<dyn Error>> {
    if !backup_dir.is_dir() {
        return Err(Box::new(io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "Backup folder does not exist: {}",
                backup_dir.display()
            ),
        )));
    }

    println!("\nImporting settings from {}\n", backup_dir.display());

    let mut copied = 0;

    for setting in settings {
        let saved_file = backup_dir.join(setting.backup_path);

        if !saved_file.is_file() {
            println!("SKIP  {} — no saved backup file", setting.tool);
            continue;
        }

        if setting.source.exists() && !force {
            println!(
                "SKIP  {} — local file already exists. Use --force to overwrite it.",
                setting.tool
            );
            continue;
        }

        let parent = setting.source.parent().ok_or_else(|| {
            io::Error::new(io::ErrorKind::Other, "Local destination has no parent folder")
        })?;

        fs::create_dir_all(parent)?;
        fs::copy(&saved_file, &setting.source)?;

        println!("COPY  {} → {}", setting.tool, setting.source.display());
        copied += 1;
    }

    println!("\nImport complete: {copied} file(s) restored.");
    println!("Restart Claude Code, Codex, and VS Code after importing.");

    Ok(())
}
