use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::{env, fs};

use eyre::{Context, bail};
use regex::Regex;

fn main() -> eyre::Result<()> {
    let Ok(home) = env::var("HOME") else {
        bail!("Could not determine home dir");
    };

    let steam_applications =
        Path::new(&home).join(".var/app/com.valvesoftware.Steam/.local/share/applications");
    let bridged_applications =
        Path::new(&home).join(".local/share/applications/steam-flatpak-bridge");
    let icon_path = Path::new(&home)
        .join(".var/app/com.valvesoftware.Steam/.local/share/icons/hicolor/32x32/apps");

    let exec_re = Regex::new(r"Exec=steam (.+)")?;
    let icon_re = Regex::new(r"Icon=(.+)")?;

    fs::create_dir_all(&bridged_applications)?;

    for entry in fs::read_dir(&steam_applications).wrap_err("Failed to read applications dir")? {
        let Ok(entry) = entry else { continue };

        if !entry.file_type()?.is_file() || !entry.file_name().as_bytes().ends_with(b".desktop") {
            continue;
        }

        let contents = fs::read_to_string(entry.path())?;
        let contents = exec_re.replace(&contents, "Exec=xdg-open $1");
        let contents = icon_re.replace(&contents, format!("Icon={}/$1.png", icon_path.display()));

        println!("{contents}");

        fs::write(
            bridged_applications.join(entry.file_name()),
            contents.as_bytes(),
        )?;
    }

    Ok(())
}
