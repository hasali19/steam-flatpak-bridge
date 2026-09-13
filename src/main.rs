use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, mpsc};
use std::{env, fs, io};

use eyre::{Context, bail};
use notify::{Event, EventKind, RecursiveMode, Watcher, recommended_watcher};
use regex::Regex;
use tracing::{error, info};

static EXEC_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"Exec=steam (.+)").unwrap());
static ICON_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"Icon=(.+)").unwrap());

fn main() -> eyre::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let Ok(home) = env::var("HOME") else {
        bail!("Could not determine home dir");
    };

    let src_dir =
        Path::new(&home).join(".var/app/com.valvesoftware.Steam/.local/share/applications");
    let target_dir = Path::new(&home).join(".local/share/applications/steam-flatpak-bridge");
    let icon_path = Path::new(&home)
        .join(".var/app/com.valvesoftware.Steam/.local/share/icons/hicolor/32x32/apps");

    fs::create_dir_all(&target_dir)?;

    let sync_manager = SyncManager {
        src_dir,
        target_dir,
        icon_path,
    };

    sync_manager.sync_all()?;
    sync_manager.watch()
}

struct SyncManager {
    src_dir: PathBuf,
    target_dir: PathBuf,
    icon_path: PathBuf,
}

impl SyncManager {
    fn sync_all(&self) -> eyre::Result<()> {
        for entry in fs::read_dir(&self.src_dir).wrap_err("Failed to read applications dir")? {
            let Ok(entry) = entry else { continue };

            if !entry.file_type()?.is_file() || !Self::is_desktop_file(&entry.path()) {
                continue;
            }

            self.sync_file(&entry.path())?;
        }

        Ok(())
    }

    fn watch(&self) -> eyre::Result<()> {
        let (tx, rx) = mpsc::channel::<notify::Result<Event>>();

        let mut watcher = recommended_watcher(tx)?;
        watcher.watch(&self.src_dir, RecursiveMode::NonRecursive)?;

        info!("Watching {} for changes", self.src_dir.display());

        for res in rx {
            let event = res.wrap_err("Watch error")?;

            for path in &event.paths {
                if !Self::is_desktop_file(path) {
                    continue;
                }

                let result = match event.kind {
                    EventKind::Create(_) | EventKind::Modify(_) => self.sync_file(path),
                    EventKind::Remove(_) => self.remove_file(path),
                    _ => continue,
                };

                if let Err(err) = result {
                    error!("{err:?}");
                }
            }
        }

        Ok(())
    }

    fn sync_file(&self, path: &Path) -> eyre::Result<()> {
        let Some(file_name) = path.file_name() else {
            return Ok(());
        };

        let contents = match fs::read_to_string(path) {
            Ok(contents) => contents,
            Err(err) => return Err(err).wrap_err_with(|| format!("Failed to read {path:?}")),
        };

        let contents = EXEC_RE.replace(&contents, "Exec=xdg-open $1");
        let contents = ICON_RE.replace(
            &contents,
            format!("Icon={}/$1.png", self.icon_path.display()),
        );

        info!("Syncing {}", path.display());

        fs::write(self.target_dir.join(file_name), contents.as_bytes())
            .wrap_err_with(|| format!("Failed to write desktop file for {path:?}"))
    }

    fn remove_file(&self, path: &Path) -> eyre::Result<()> {
        let Some(file_name) = path.file_name() else {
            return Ok(());
        };

        let target_path = self.target_dir.join(file_name);

        info!("Removing {}", target_path.display());

        match fs::remove_file(&target_path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err).wrap_err_with(|| format!("Failed to remove {target_path:?}")),
        }
    }

    fn is_desktop_file(path: &Path) -> bool {
        path.file_name()
            .is_some_and(|name| name.as_bytes().ends_with(b".desktop"))
    }
}
