use crate::core::{CoreManager, handle, manager::RunningMode};
use anyhow::Result;
use async_trait::async_trait;
use clash_verge_logging::{Type, logging};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::OnceLock,
};
use tauri::Manager as _;

#[cfg(not(feature = "verge-dev"))]
pub static APP_ID: &str = "io.github.clash-verge-rev.clash-verge-rev";
#[cfg(not(feature = "verge-dev"))]
pub static BACKUP_DIR: &str = "clash-verge-rev-backup";

#[cfg(feature = "verge-dev")]
pub static APP_ID: &str = "io.github.clash-verge-rev.clash-verge-rev.dev";
#[cfg(feature = "verge-dev")]
pub static BACKUP_DIR: &str = "clash-verge-rev-backup-dev";

pub static CLASH_CONFIG: &str = "config.yaml";
pub static VERGE_CONFIG: &str = "verge.yaml";
pub static PROFILE_YAML: &str = "profiles.yaml";
/// Marks that the one-shot raise of too-short auto-update intervals has already run.
pub static UPDATE_INTERVAL_MIGRATED: &str = ".update-interval-migrated";

/// Name of the portable configuration directory, located next to the executable.
pub static PORTABLE_CONFIG_DIR: &str = "config";

/// Cached result of [`resolve_portable_home_dir`].
///
/// Resolved lazily on first use: the answer depends on the filesystem and cannot change while the
/// process lives, whereas `app_home_dir()` sits on hot paths that must not touch the disk.
static PORTABLE_HOME_DIR: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Resolves `<directory of the executable>/config`, creating the directory when it is missing.
///
/// Returns `None` when that location cannot be created or written to, so the caller falls back to
/// the system data directory. This keeps installed builds usable from locations where a standard
/// user may not create files next to the executable, such as `Program Files`.
fn resolve_portable_home_dir() -> Option<PathBuf> {
    let dir = std::env::current_exe().ok()?.parent()?.join(PORTABLE_CONFIG_DIR);

    // Idempotent when the directory already exists — which is also the "read existing config" path.
    if let Err(error) = fs::create_dir_all(&dir) {
        logging!(warn, Type::Setup, "便携配置目录不可用，改用系统数据目录: {error:#}");
        return None;
    }

    // A directory may exist yet still reject new files (read-only media, locked-down ACL).
    let probe = dir.join(".portable-write-probe");
    match fs::File::create(&probe) {
        Ok(_) => {
            let _ = fs::remove_file(&probe);
            Some(dir)
        }
        Err(error) => {
            logging!(warn, Type::Setup, "便携配置目录不可写，改用系统数据目录: {error:#}");
            None
        }
    }
}

/// Uses the same platform data resolver as Tauri, including before its handle exists.
///
/// Portable layout: all configuration lives in `config/` next to the executable, so the entire
/// installation can be moved together with its settings. Missing files are created from templates
/// by the normal initialization path, so an absent `config/` simply starts from defaults.
pub fn app_home_dir() -> Result<PathBuf> {
    if let Some(dir) = PORTABLE_HOME_DIR.get_or_init(resolve_portable_home_dir) {
        return Ok(dir.clone());
    }

    ::dirs::data_dir()
        .map(|root| root.join(APP_ID))
        .ok_or_else(|| anyhow::anyhow!("Failed to get the app home directory"))
}

pub fn preinit_app_data_dir() -> Result<PathBuf> {
    app_home_dir()
}

pub fn app_resources_dir() -> Result<PathBuf> {
    let app_handle = handle::Handle::app_handle();

    match app_handle.path().resource_dir() {
        Ok(dir) => Ok(dir.join("resources")),
        Err(e) => {
            logging!(error, Type::File, "Failed to get the resource directory: {e}");
            Err(anyhow::anyhow!("Failed to get the resource directory"))
        }
    }
}

pub fn app_profiles_dir() -> Result<PathBuf> {
    Ok(app_home_dir()?.join("profiles"))
}

pub fn app_icons_dir() -> Result<PathBuf> {
    Ok(app_home_dir()?.join("icons"))
}

pub fn find_target_icons(target: &str) -> Result<Option<String>> {
    let icons_dir = app_icons_dir()?;
    let icon_path = fs::read_dir(&icons_dir)?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .find(|path| {
            let prefix_matches = path
                .file_prefix()
                .and_then(|p| p.to_str())
                .is_some_and(|prefix| prefix.starts_with(target));
            let ext_matches = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("ico") || ext.eq_ignore_ascii_case("png"));
            prefix_matches && ext_matches
        });

    icon_path.map(|path| path_to_str(&path).map(|s| s.into())).transpose()
}

pub fn app_logs_dir() -> Result<PathBuf> {
    Ok(app_home_dir()?.join("logs"))
}

#[cfg(target_os = "macos")]
pub fn service_logs_root_dir() -> Result<PathBuf> {
    Ok(app_home_dir()?.join("service-logs"))
}

#[cfg(not(target_os = "macos"))]
pub fn service_logs_root_dir() -> Result<PathBuf> {
    app_logs_dir()
}

pub fn app_latest_log() -> Result<PathBuf> {
    Ok(app_logs_dir()?.join("latest.log"))
}

pub fn local_backup_dir() -> Result<PathBuf> {
    let dir = app_home_dir()?.join(BACKUP_DIR);
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn clash_path() -> Result<PathBuf> {
    Ok(app_home_dir()?.join(CLASH_CONFIG))
}

pub fn verge_path() -> Result<PathBuf> {
    Ok(app_home_dir()?.join(VERGE_CONFIG))
}

pub fn profiles_path() -> Result<PathBuf> {
    Ok(app_home_dir()?.join(PROFILE_YAML))
}

pub fn update_interval_migrated_path() -> Result<PathBuf> {
    Ok(app_home_dir()?.join(UPDATE_INTERVAL_MIGRATED))
}

#[cfg(target_os = "macos")]
pub fn service_path() -> Result<PathBuf> {
    let res_dir = app_resources_dir()?;
    Ok(res_dir.join("clash-verge-service"))
}

#[cfg(windows)]
pub fn service_path() -> Result<PathBuf> {
    let res_dir = app_resources_dir()?;
    Ok(res_dir.join("clash-verge-service.exe"))
}

pub fn sidecar_log_dir() -> Result<PathBuf> {
    let log_dir = app_logs_dir()?.join("sidecar");
    let _ = std::fs::create_dir_all(&log_dir);

    Ok(log_dir)
}

pub fn service_log_dir() -> Result<PathBuf> {
    let log_dir = service_logs_root_dir()?.join("service");
    let _ = std::fs::create_dir_all(&log_dir);

    Ok(log_dir)
}

pub fn clash_latest_log() -> Result<PathBuf> {
    match *CoreManager::global().get_running_mode() {
        RunningMode::Service => Ok(service_log_dir()?.join("service_latest.log")),
        RunningMode::Sidecar | RunningMode::NotRunning => Ok(sidecar_log_dir()?.join("sidecar_latest.log")),
    }
}

pub fn path_to_str(path: &Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| anyhow::anyhow!("failed to get path from {:?}", path))
}

pub fn get_encryption_key() -> Result<Vec<u8>> {
    let app_dir = app_home_dir()?;
    let key_path = app_dir.join(".encryption_key");

    if key_path.exists() {
        fs::read(&key_path).map_err(|e| anyhow::anyhow!("Failed to read encryption key: {}", e))
    } else {
        let mut key = vec![0u8; 32];
        getrandom::fill(&mut key)?;

        if let Some(parent) = key_path.parent() {
            fs::create_dir_all(parent).map_err(|e| anyhow::anyhow!("Failed to create key directory: {}", e))?;
        }
        fs::write(&key_path, &key).map_err(|e| anyhow::anyhow!("Failed to save encryption key: {}", e))?;
        Ok(key)
    }
}

pub fn ipc_path() -> Result<PathBuf> {
    Ok(PathBuf::from(clash_verge_service_ipc::mihomo_ipc_path(
        &crate::core::owner_identity::current_owner_identity()?,
    )))
}

#[cfg(target_os = "macos")]
pub fn sidecar_ipc_path() -> Result<PathBuf> {
    sidecar_ipc_path_for(
        std::path::Path::new(""),
        &crate::core::owner_identity::current_owner_identity()?,
    )
}

#[cfg(not(target_os = "macos"))]
pub fn sidecar_ipc_path() -> Result<PathBuf> {
    Ok(sidecar_ipc_path_for(
        &preinit_app_data_dir()?,
        &crate::core::owner_identity::current_owner_identity()?,
    ))
}

#[cfg(target_os = "linux")]
fn sidecar_ipc_path_for(app_root: &std::path::Path, _identity: &clash_verge_service_ipc::OwnerIdentity) -> PathBuf {
    app_root.join("verge-mihomo.sock")
}

#[cfg(target_os = "macos")]
fn sidecar_ipc_path_for(
    _app_root: &std::path::Path,
    _identity: &clash_verge_service_ipc::OwnerIdentity,
) -> Result<PathBuf> {
    use std::{ffi::OsStr, os::unix::ffi::OsStrExt as _};

    // SAFETY: A null buffer with size zero asks confstr for the required buffer length.
    let required_len = unsafe { libc::confstr(libc::_CS_DARWIN_USER_TEMP_DIR, std::ptr::null_mut(), 0) };
    if required_len == 0 {
        return Err(anyhow::anyhow!("macOS per-user temporary directory is unavailable"));
    }

    let mut buffer = vec![0_u8; required_len];
    // SAFETY: buffer is writable for buffer.len() bytes, as required by confstr.
    let written = unsafe { libc::confstr(libc::_CS_DARWIN_USER_TEMP_DIR, buffer.as_mut_ptr().cast(), buffer.len()) };
    if written == 0 || written > buffer.len() {
        return Err(anyhow::anyhow!("failed to read macOS per-user temporary directory"));
    }

    let root = std::ffi::CStr::from_bytes_until_nul(&buffer)
        .map_err(|_| anyhow::anyhow!("macOS per-user temporary directory is not NUL-terminated"))?;
    #[cfg(feature = "verge-dev")]
    let filename = "verge-mihomo-dev.sock";
    #[cfg(not(feature = "verge-dev"))]
    let filename = "verge-mihomo.sock";
    let path = PathBuf::from(OsStr::from_bytes(root.to_bytes())).join(filename);

    let path_len = path.as_os_str().as_bytes().len();
    if path_len >= 104 {
        return Err(anyhow::anyhow!(
            "macOS Sidecar IPC path is {path_len} bytes, but sockaddr_un.sun_path requires fewer than 104 bytes: {:?}",
            path,
        ));
    }

    Ok(path)
}

#[cfg(windows)]
fn sidecar_ipc_path_for(_app_root: &std::path::Path, identity: &clash_verge_service_ipc::OwnerIdentity) -> PathBuf {
    PathBuf::from(sidecar_pipe_name(identity, cfg!(feature = "verge-dev")))
}

#[cfg(any(windows, test))]
fn sidecar_pipe_name(identity: &clash_verge_service_ipc::OwnerIdentity, is_dev: bool) -> String {
    let flavor = if is_dev { "dev" } else { "release" };
    format!(
        r"\\.\pipe\verge-mihomo-sidecar-{flavor}-{}",
        clash_verge_service_ipc::owner_key(identity)
    )
}

#[cfg(all(test, target_os = "linux"))]
mod ipc_tests {
    use super::sidecar_ipc_path_for;
    use clash_verge_service_ipc::OwnerIdentity;
    use std::path::Path;

    #[test]
    fn sidecar_ipc_stays_in_the_app_root() {
        let identity = OwnerIdentity::Unix { uid: 501, gid: 20 };
        let app_root = Path::new("/home/test/.local/share/io.github.clash-verge-rev.clash-verge-rev");
        let path = sidecar_ipc_path_for(app_root, &identity);

        assert_eq!(path, app_root.join("verge-mihomo.sock"));
        assert_ne!(
            path.to_string_lossy(),
            clash_verge_service_ipc::mihomo_ipc_path(&identity)
        );
    }
}

#[cfg(all(test, target_os = "macos"))]
mod ipc_tests {
    use super::sidecar_ipc_path_for;
    use clash_verge_service_ipc::OwnerIdentity;
    use std::{ffi::OsStr, os::unix::ffi::OsStrExt as _, path::Path};

    #[test]
    fn sidecar_ipc_ignores_long_app_root_and_fits_sockaddr_un() -> anyhow::Result<()> {
        let identity = OwnerIdentity::Unix { uid: 501, gid: 20 };
        let app_root =
            Path::new("/Users/support/Library/Application Support/io.github.clash-verge-rev.clash-verge-rev.dev");
        let path = sidecar_ipc_path_for(app_root, &identity)?;

        assert!(!path.starts_with(app_root));
        assert!(path.as_os_str().as_bytes().len() < 104);
        #[cfg(feature = "verge-dev")]
        assert_eq!(path.file_name(), Some(OsStr::new("verge-mihomo-dev.sock")));
        #[cfg(not(feature = "verge-dev"))]
        assert_eq!(path.file_name(), Some(OsStr::new("verge-mihomo.sock")));
        assert_eq!(path, sidecar_ipc_path_for(Path::new("/different/root"), &identity)?);
        assert!(path.parent().is_some_and(Path::is_dir));
        Ok(())
    }
}

#[cfg(all(test, windows))]
mod ipc_tests {
    use super::sidecar_ipc_path_for;
    use clash_verge_service_ipc::OwnerIdentity;
    use std::path::Path;

    #[test]
    fn sidecar_ipc_uses_the_current_owners_named_pipe() {
        let identity = OwnerIdentity::Windows {
            sid: "S-1-5-21-1000".to_owned(),
        };
        let path = sidecar_ipc_path_for(Path::new(r"C:\ignored"), &identity);

        assert_eq!(
            path,
            Path::new(&format!(
                r"\\.\pipe\verge-mihomo-sidecar-{}-{}",
                if cfg!(feature = "verge-dev") { "dev" } else { "release" },
                clash_verge_service_ipc::owner_key(&identity)
            ))
        );
    }
}

#[async_trait]
pub trait PathBufExec {
    async fn remove_if_exists(&self) -> Result<()>;
}

#[async_trait]
impl PathBufExec for PathBuf {
    async fn remove_if_exists(&self) -> Result<()> {
        if self.exists() {
            tokio::fs::remove_file(self).await?;
            logging!(debug, Type::File, "Removed file: {:?}", self);
        }
        Ok(())
    }
}

#[cfg(test)]
mod windows_pipe_name_tests {
    use super::sidecar_pipe_name;
    use clash_verge_service_ipc::OwnerIdentity;

    #[test]
    fn windows_sidecar_pipe_separates_dev_and_release_for_the_same_owner() {
        let identity = OwnerIdentity::Windows {
            sid: "S-1-5-21-1000".to_owned(),
        };
        let owner_key = clash_verge_service_ipc::owner_key(&identity);

        assert_eq!(
            sidecar_pipe_name(&identity, false),
            format!(r"\\.\pipe\verge-mihomo-sidecar-release-{owner_key}")
        );
        assert_eq!(
            sidecar_pipe_name(&identity, true),
            format!(r"\\.\pipe\verge-mihomo-sidecar-dev-{owner_key}")
        );
    }
}
