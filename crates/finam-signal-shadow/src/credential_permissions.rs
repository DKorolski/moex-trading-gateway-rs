//! A group-readable credential is accepted only on the pinned read-only
//! systemd credential mount, never as an arbitrary 0440 secret file.
use std::path::Path;

const DIRECTORY: &str = "/run/credentials/signal-shadow.service";
const FILE: &str = "/run/credentials/signal-shadow.service/finam-read-only-secret";

#[cfg(test)]
fn protected_mount(text: &str) -> bool {
    protected_mount_at(text, DIRECTORY)
}

fn protected_mount_at(text: &str, directory: &str) -> bool {
    let mut matches = text.lines().filter_map(|line| {
        let (left, right) = line.split_once(" - ")?;
        let fields: Vec<_> = left.split_whitespace().collect();
        (fields.get(4) == Some(&directory)).then_some((fields, right))
    });
    let Some((fields, right)) = matches.next() else {
        return false;
    };
    matches.next().is_none()
        && fields.get(5).is_some_and(|options| {
            options.split(',').any(|v| v == "ro") && !options.split(',').any(|v| v == "rw")
        })
        && matches!(right.split_whitespace().next(), Some("tmpfs" | "ramfs"))
}

fn protected_metadata(
    file_mode: u32,
    file_uid: u32,
    file_gid: u32,
    dir_mode: u32,
    dir_uid: u32,
    dir_gid: u32,
) -> bool {
    file_mode == 0o440
        && file_uid == 0
        && file_gid == 0
        && dir_mode == 0o550
        && dir_uid == 0
        && dir_gid == 0
}

pub(crate) fn validate(path: &Path) -> Result<(), &'static str> {
    validate_at(path, DIRECTORY, FILE)
}

pub(crate) fn validate_diagnostic(path: &Path) -> Result<(), &'static str> {
    validate_at(
        path,
        "/run/credentials/signal-shadow-auth-ws-diagnostic.service",
        "/run/credentials/signal-shadow-auth-ws-diagnostic.service/finam-read-only-secret",
    )
}

#[cfg(feature = "models-bridge")]
pub(crate) fn validate_usd_ri(path: &Path) -> Result<(), &'static str> {
    validate_at(
        path,
        "/run/credentials/usd-ri-shadow.service",
        "/run/credentials/usd-ri-shadow.service/finam-read-only-secret",
    )
}

fn validate_at(path: &Path, directory: &str, file: &str) -> Result<(), &'static str> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let meta = std::fs::symlink_metadata(path).map_err(|_| "credential_io")?;
        if !meta.is_file() {
            return Err("credential_permissions");
        }
        if meta.mode() & 0o077 == 0 {
            return Ok(());
        }
        // Only Linux systemd provides this exact mount contract. The environment
        // alone is not proof: check path, owner, mode and the actual VFS mount.
        if !cfg!(target_os = "linux")
            || path.as_os_str() != std::ffi::OsStr::new(file)
            || std::env::var_os("CREDENTIALS_DIRECTORY").as_deref()
                != Some(std::ffi::OsStr::new(directory))
        {
            return Err("credential_permissions");
        }
        let dir = std::fs::symlink_metadata(directory).map_err(|_| "credential_permissions")?;
        if !dir.is_dir()
            || !protected_metadata(
                meta.mode() & 0o7777,
                meta.uid(),
                meta.gid(),
                dir.mode() & 0o7777,
                dir.uid(),
                dir.gid(),
            )
            || std::fs::canonicalize(path).map_err(|_| "credential_permissions")? != path
        {
            return Err("credential_permissions");
        }
        let mounts = std::fs::read_to_string("/proc/self/mountinfo")
            .map_err(|_| "credential_permissions")?;
        if !protected_mount_at(&mounts, directory) {
            return Err("credential_permissions");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    const MOUNT: &str = "443 442 0:52 / /run/credentials/signal-shadow.service ro,nosuid,nodev,noexec,relatime,nosymfollow shared:381 master:365 - tmpfs tmpfs rw,size=1024k";
    #[test]
    fn captured_systemd_readonly_mount_is_supported() {
        // Superblock rw is normal; per-mount ro is the access restriction.
        assert!(protected_mount(MOUNT));
        assert!(protected_metadata(0o440, 0, 0, 0o550, 0, 0));
    }
    #[test]
    fn diagnostic_and_production_mount_bindings_do_not_cross() {
        let directory = "/run/credentials/signal-shadow-auth-ws-diagnostic.service";
        let mount = MOUNT.replace(DIRECTORY, directory);
        assert!(protected_mount_at(&mount, directory));
        assert!(!protected_mount_at(MOUNT, directory));
        assert!(!protected_mount(&mount));
        assert!(!protected_mount_at(
            &mount.replace(directory, "/run/credentials/untrusted.service"),
            directory
        ));
    }
    #[test]
    fn usd_ri_mount_is_exact_and_does_not_cross_existing_namespaces() {
        let directory = "/run/credentials/usd-ri-shadow.service";
        let mount = MOUNT.replace(DIRECTORY, directory);
        assert!(protected_mount_at(&mount, directory));
        assert!(!protected_mount_at(MOUNT, directory));
        assert!(!protected_mount(&mount));
        assert!(!protected_mount_at(
            &mount,
            "/run/credentials/signal-shadow-auth-ws-diagnostic.service"
        ));
    }
    #[test]
    fn writable_wrong_or_ambiguous_mounts_are_denied() {
        assert!(!protected_mount(&MOUNT.replace("ro,nosuid", "rw,nosuid")));
        assert!(!protected_mount(
            &MOUNT.replace("signal-shadow.service", "other.service")
        ));
        assert!(!protected_mount(&MOUNT.replace("- tmpfs", "- ext4")));
        assert!(!protected_mount(&format!("{MOUNT}\n{MOUNT}")));
        assert!(!protected_mount(""));
    }
    #[test]
    fn permission_broadening_is_denied() {
        for mode in [0o444, 0o640, 0o660, 0o1440, 0o2440] {
            assert!(!protected_metadata(mode, 0, 0, 0o550, 0, 0));
        }
        assert!(!protected_metadata(0o440, 999, 0, 0o550, 0, 0));
        assert!(!protected_metadata(0o440, 0, 996, 0o550, 0, 0));
        assert!(!protected_metadata(0o440, 0, 0, 0o750, 0, 0));
    }
    #[cfg(unix)]
    #[test]
    fn private_file_unchanged_but_arbitrary_group_file_and_symlink_denied() {
        use std::os::unix::fs::PermissionsExt;
        let p = std::env::temp_dir().join(format!(
            "shadow-credential-permissions-{}",
            std::process::id()
        ));
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        let file = options.open(&p).unwrap();
        file.set_permissions(std::fs::Permissions::from_mode(0o600))
            .unwrap();
        assert!(validate(&p).is_ok());
        file.set_permissions(std::fs::Permissions::from_mode(0o440))
            .unwrap();
        assert_eq!(validate(&p), Err("credential_permissions"));
        let link = p.with_extension("link");
        std::os::unix::fs::symlink(&p, &link).unwrap();
        assert_eq!(validate(&link), Err("credential_permissions"));
        std::fs::remove_file(link).unwrap();
        std::fs::remove_file(p).unwrap();
    }
}
