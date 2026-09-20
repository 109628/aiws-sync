# ai-workspace-sync

`aiws` backs up portable Claude Code, Codex CLI, and VS Code settings.

## Export to GitHub

Create an empty **private** GitHub repository for your settings, copy its clone URL,
and run:

```powershell
aiws export --repo https://github.com/YOUR-USER/aiws-backup.git
```

The command initializes the local backup directory, copies supported settings,
creates a commit when anything changed, and pushes `main` to the supplied remote.

By default, settings are stored in `%USERPROFILE%\\.aiws-backup`. Use `--dir` to
choose another local backup directory when supplying a Git URL.

```powershell
aiws export --repo https://github.com/YOUR-USER/aiws-backup.git --dir D:\backups\aiws
```

## Import

Clone the private backup repository to the local backup directory, then restore it:

```powershell
aiws import --force
```

`--force` replaces existing local configuration files.

## Security

The tool creates a `.gitignore` for common secret and runtime-data patterns. Review
every exported file before its first push, and keep your settings backup repository
private.
