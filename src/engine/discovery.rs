use anyhow::{bail, Result};
use std::env;
use std::path::{Path, PathBuf};

pub const STOCKFISH_PATH_ENV: &str = "STOCKFISH_PATH";

/// Locate a usable Stockfish executable without relying on machine-specific paths.
///
/// An explicit `STOCKFISH_PATH` takes precedence. Otherwise, the executable is
/// searched for next to the app, in the current directory, in common install
/// directories, and on `PATH`.
pub fn discover_stockfish() -> Result<PathBuf> {
    let current_executable = env::current_exe()
        .ok()
        .map(|path| canonical_or_original(&path));

    if let Some(configured) = env::var_os(STOCKFISH_PATH_ENV).filter(|value| !value.is_empty()) {
        let configured = expand_home(PathBuf::from(configured));
        if let Some(path) = resolve_configured_path(&configured, current_executable.as_deref()) {
            return Ok(path);
        }

        bail!(
            "{STOCKFISH_PATH_ENV} points to '{}', but it is not an executable file",
            configured.display()
        );
    }

    let mut directories = Vec::new();

    if let Some(executable) = &current_executable {
        if let Some(parent) = executable.parent() {
            directories.push(parent.to_path_buf());
        }
    }

    if let Ok(current_dir) = env::current_dir() {
        directories.push(current_dir);
    }

    if let Some(home) = dirs::home_dir() {
        directories.push(home.join("bin"));
    }

    #[cfg(not(target_os = "windows"))]
    {
        directories.extend([
            PathBuf::from("/opt/homebrew/bin"),
            PathBuf::from("/usr/local/bin"),
            PathBuf::from("/usr/bin"),
        ]);
    }

    if let Some(path) = env::var_os("PATH") {
        directories.extend(env::split_paths(&path));
    }

    for directory in directories {
        if let Some(path) = find_in_directory(&directory, current_executable.as_deref()) {
            return Ok(path);
        }
    }

    bail!(
        "Stockfish was not found.\n\n\
         To fix this, you can:\n\
         1. Install via package manager (e.g., `brew install stockfish` on macOS, \
            `apt install stockfish` on Debian/Ubuntu)\n\
         2. Download from stockfishchess.org and place in ~/bin or /usr/local/bin\n\
         3. Set {STOCKFISH_PATH_ENV} to the full path of the Stockfish executable\n\n\
         Searched locations: next to app, current directory, ~/bin, \
         /opt/homebrew/bin, /usr/local/bin, /usr/bin, and PATH directories."
    )
}

fn resolve_configured_path(configured: &Path, excluded: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = usable_candidate(configured, excluded) {
        return Some(path);
    }

    if configured.components().count() == 1 {
        if let Some(path) = env::var_os("PATH") {
            for directory in env::split_paths(&path) {
                let candidate = directory.join(configured);
                if let Some(path) = usable_candidate(&candidate, excluded) {
                    return Some(path);
                }
            }
        }
    }

    None
}

fn find_in_directory(directory: &Path, excluded: Option<&Path>) -> Option<PathBuf> {
    for name in executable_names() {
        let candidate = directory.join(name);
        if let Some(path) = usable_candidate(&candidate, excluded) {
            return Some(path);
        }
    }

    // Official downloads include platform details in the filename. Accept
    // those without forcing users to rename the binary.
    let mut downloaded_binaries = std::fs::read_dir(directory)
        .ok()?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.to_ascii_lowercase().starts_with("stockfish"))
                && usable_candidate(path, excluded).is_some()
        })
        .collect::<Vec<_>>();
    downloaded_binaries.sort();

    downloaded_binaries
        .first()
        .map(|path| canonical_or_original(path))
}

fn usable_candidate(path: &Path, excluded: Option<&Path>) -> Option<PathBuf> {
    if !is_usable_executable(path) {
        return None;
    }

    let candidate = canonical_or_original(path);
    (!excluded.is_some_and(|excluded| candidate == excluded)).then_some(candidate)
}

fn executable_names() -> &'static [&'static str] {
    #[cfg(target_os = "windows")]
    {
        &["stockfish.exe", "stockfish"]
    }

    #[cfg(not(target_os = "windows"))]
    {
        &["stockfish"]
    }
}

fn expand_home(path: PathBuf) -> PathBuf {
    let Ok(remainder) = path.strip_prefix("~") else {
        return path;
    };

    dirs::home_dir()
        .map(|home| home.join(remainder))
        .unwrap_or(path)
}

fn canonical_or_original(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(unix)]
fn is_usable_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    path.metadata()
        .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_usable_executable(path: &Path) -> bool {
    path.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_directory() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = env::temp_dir().join(format!(
            "stockfish-chess-discovery-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn create_executable(path: &Path) {
        fs::write(path, b"test engine").unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let mut permissions = fs::metadata(path).unwrap().permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(path, permissions).unwrap();
        }
    }

    #[test]
    fn finds_official_download_name() {
        let directory = temporary_directory();
        let executable = directory.join("stockfish-macos-arm64");
        create_executable(&executable);

        assert_eq!(
            find_in_directory(&directory, None),
            Some(executable.canonicalize().unwrap())
        );

        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn prefers_plain_stockfish_name() {
        let directory = temporary_directory();
        let downloaded = directory.join("stockfish-custom");
        let plain = directory.join(executable_names()[0]);
        create_executable(&downloaded);
        create_executable(&plain);

        assert_eq!(
            find_in_directory(&directory, None),
            Some(plain.canonicalize().unwrap())
        );

        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn ignores_non_executable_files() {
        let directory = temporary_directory();
        fs::write(directory.join(executable_names()[0]), b"not executable").unwrap();

        #[cfg(unix)]
        assert_eq!(find_in_directory(&directory, None), None);

        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn excludes_the_application_executable() {
        let directory = temporary_directory();
        let application = directory.join("stockfish-chess");
        create_executable(&application);
        let application = application.canonicalize().unwrap();

        assert_eq!(find_in_directory(&directory, Some(&application)), None);

        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn expand_home_expands_tilde() {
        let path = PathBuf::from("~/bin/stockfish");
        let expanded = expand_home(path);

        if let Some(home) = dirs::home_dir() {
            assert_eq!(expanded, home.join("bin/stockfish"));
        }
    }

    #[test]
    fn expand_home_preserves_absolute_paths() {
        let path = PathBuf::from("/usr/local/bin/stockfish");
        let expanded = expand_home(path.clone());
        assert_eq!(expanded, path);
    }

    #[test]
    fn expand_home_preserves_relative_paths() {
        let path = PathBuf::from("./stockfish");
        let expanded = expand_home(path.clone());
        assert_eq!(expanded, path);
    }

    #[test]
    fn finds_stockfish_with_various_official_names() {
        let directory = temporary_directory();

        let names = [
            "stockfish-ubuntu-x86-64",
            "stockfish-macos-m1-apple-silicon",
            "stockfish-windows-x86-64.exe",
            "Stockfish-16.1",
        ];

        for name in names {
            let executable = directory.join(name);
            create_executable(&executable);
            let found = find_in_directory(&directory, None);
            assert!(found.is_some(), "Should find {}", name);
            fs::remove_file(&executable).unwrap();
        }

        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn does_not_find_non_stockfish_executables() {
        let directory = temporary_directory();
        let other = directory.join("chess-engine");
        create_executable(&other);

        assert_eq!(find_in_directory(&directory, None), None);

        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn ignores_directories_named_stockfish() {
        let directory = temporary_directory();
        let stockfish_dir = directory.join("stockfish");
        fs::create_dir_all(&stockfish_dir).unwrap();

        assert_eq!(find_in_directory(&directory, None), None);

        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn resolve_configured_path_finds_in_path_env() {
        let directory = temporary_directory();
        let executable = directory.join("stockfish");
        create_executable(&executable);

        env::set_var("PATH", directory.to_str().unwrap());
        let result = resolve_configured_path(Path::new("stockfish"), None);
        assert!(result.is_some());
        env::remove_var("PATH");

        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn usable_candidate_rejects_missing_file() {
        let nonexistent = PathBuf::from("/nonexistent/stockfish");
        assert_eq!(usable_candidate(&nonexistent, None), None);
    }

    #[cfg(unix)]
    #[test]
    fn is_usable_executable_checks_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let directory = temporary_directory();

        let no_exec = directory.join("stockfish-no-exec");
        fs::write(&no_exec, b"engine").unwrap();
        let mut perms = fs::metadata(&no_exec).unwrap().permissions();
        perms.set_mode(0o644);
        fs::set_permissions(&no_exec, perms).unwrap();
        assert!(!is_usable_executable(&no_exec));

        let with_exec = directory.join("stockfish-with-exec");
        fs::write(&with_exec, b"engine").unwrap();
        let mut perms = fs::metadata(&with_exec).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&with_exec, perms).unwrap();
        assert!(is_usable_executable(&with_exec));

        fs::remove_dir_all(directory).unwrap();
    }
}
