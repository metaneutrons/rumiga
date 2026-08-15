// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

use std::ffi::OsStr;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::io::AsyncWriteExt;

pub const DEFAULT_STORAGE_ROOT: &str = "rumiga-media";
pub const DEFAULT_UPLOAD_LIMIT_BYTES: u64 = 2 * 1024 * 1024 * 1024;
pub const MAX_UPLOAD_LIMIT_BYTES: u64 = 8 * 1024 * 1024 * 1024;

const UPLOAD_EXTENSIONS: &[&str] = &["adf", "adz", "hdf", "rom"];
const UPLOAD_TEMP_PREFIX: &str = ".rumiga-upload-";
static UPLOAD_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
pub enum StorageError {
    InvalidConfiguration,
    InvalidPath,
    AccessDenied,
    NotFound,
    NotDirectory,
    NotFile,
    UnsupportedMediaType,
    AlreadyExists,
    UploadTooLarge { limit_bytes: u64 },
    Io(io::Error),
}

impl fmt::Display for StorageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfiguration => formatter.write_str("invalid storage configuration"),
            Self::InvalidPath => formatter.write_str("invalid storage-relative path"),
            Self::AccessDenied => formatter.write_str("path escapes the configured storage root"),
            Self::NotFound => formatter.write_str("storage entry not found"),
            Self::NotDirectory => formatter.write_str("storage entry is not a directory"),
            Self::NotFile => formatter.write_str("storage entry is not a regular file"),
            Self::UnsupportedMediaType => formatter.write_str("unsupported media file type"),
            Self::AlreadyExists => formatter.write_str("storage entry already exists"),
            Self::UploadTooLarge { limit_bytes } => {
                write!(formatter, "upload exceeds the {limit_bytes}-byte limit")
            }
            Self::Io(error) => write!(formatter, "storage I/O failed: {error}"),
        }
    }
}

impl std::error::Error for StorageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for StorageError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Clone, Debug)]
pub struct MediaStore {
    root: PathBuf,
    upload_limit_bytes: u64,
}

impl MediaStore {
    pub fn new(root: impl AsRef<Path>, upload_limit_bytes: u64) -> Result<Self, StorageError> {
        let root = root.as_ref();
        if root.as_os_str().is_empty()
            || upload_limit_bytes == 0
            || upload_limit_bytes > MAX_UPLOAD_LIMIT_BYTES
        {
            return Err(StorageError::InvalidConfiguration);
        }

        fs::create_dir_all(root).map_err(StorageError::Io)?;
        let root = fs::canonicalize(root).map_err(StorageError::Io)?;
        if !fs::metadata(&root).map_err(StorageError::Io)?.is_dir() {
            return Err(StorageError::NotDirectory);
        }

        Ok(Self {
            root,
            upload_limit_bytes,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub const fn upload_limit_bytes(&self) -> u64 {
        self.upload_limit_bytes
    }

    pub fn list(&self, virtual_path: &str) -> Result<rumiga_api::FileListResponse, StorageError> {
        let relative = validated_relative_path(virtual_path, true)?;
        let directory = self.resolve_existing(&relative)?;
        if !fs::metadata(&directory)
            .map_err(map_existing_error)?
            .is_dir()
        {
            return Err(StorageError::NotDirectory);
        }

        let mut files = Vec::new();
        for entry in fs::read_dir(&directory).map_err(map_existing_error)? {
            let entry = entry.map_err(StorageError::Io)?;
            let file_type = entry.file_type().map_err(StorageError::Io)?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if file_type.is_symlink() || name.starts_with(UPLOAD_TEMP_PREFIX) {
                continue;
            }
            if file_type.is_dir() {
                files.push(rumiga_api::FileEntry {
                    name,
                    size: 0,
                    is_directory: true,
                });
            } else if file_type.is_file() {
                files.push(rumiga_api::FileEntry {
                    name,
                    size: entry.metadata().map_err(StorageError::Io)?.len(),
                    is_directory: false,
                });
            }
        }
        files.sort_by(|left, right| {
            right
                .is_directory
                .cmp(&left.is_directory)
                .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
                .then_with(|| left.name.cmp(&right.name))
        });

        Ok(rumiga_api::FileListResponse {
            path: display_virtual_path(&relative),
            files,
            total_bytes: fs2::total_space(&self.root).map_err(StorageError::Io)?,
            free_bytes: fs2::available_space(&self.root).map_err(StorageError::Io)?,
        })
    }

    pub fn resolve_file(
        &self,
        virtual_path: &str,
        allowed_extensions: &[&str],
    ) -> Result<PathBuf, StorageError> {
        let relative = validated_relative_path(virtual_path, false)?;
        require_extension(&relative, allowed_extensions)?;
        let path = self.resolve_existing(&relative)?;
        if !fs::metadata(&path).map_err(map_existing_error)?.is_file() {
            return Err(StorageError::NotFile);
        }
        Ok(path)
    }

    pub async fn read_file(
        &self,
        virtual_path: &str,
        allowed_extensions: &[&str],
    ) -> Result<(PathBuf, Vec<u8>), StorageError> {
        let path = self.resolve_file(virtual_path, allowed_extensions)?;
        let bytes = tokio::fs::read(&path).await.map_err(map_existing_error)?;
        Ok((path, bytes))
    }

    pub fn delete_file(&self, virtual_path: &str) -> Result<(), StorageError> {
        let relative = validated_relative_path(virtual_path, false)?;
        require_extension(&relative, UPLOAD_EXTENSIONS)?;
        let target = self.resolve_existing(&relative)?;
        if !fs::metadata(&target).map_err(map_existing_error)?.is_file() {
            return Err(StorageError::NotFile);
        }
        fs::remove_file(target).map_err(map_existing_error)
    }

    pub async fn begin_upload(&self, file_name: &str) -> Result<PendingUpload, StorageError> {
        let relative = validated_relative_path(file_name, false)?;
        if relative.components().count() != 1 {
            return Err(StorageError::InvalidPath);
        }
        require_extension(&relative, UPLOAD_EXTENSIONS)?;
        let destination = self.root.join(&relative);
        match fs::symlink_metadata(&destination) {
            Ok(_) => return Err(StorageError::AlreadyExists),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(StorageError::Io(error)),
        }

        for _ in 0..32 {
            let sequence = UPLOAD_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let temp_path = self.root.join(format!(
                "{UPLOAD_TEMP_PREFIX}{}-{sequence}.tmp",
                std::process::id()
            ));
            let file = tokio::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp_path)
                .await;
            match file {
                Ok(file) => {
                    return Ok(PendingUpload {
                        destination,
                        temp_path: Some(temp_path),
                        file: Some(file),
                        bytes_written: 0,
                        limit_bytes: self.upload_limit_bytes,
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(StorageError::Io(error)),
            }
        }

        Err(StorageError::Io(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "could not allocate a unique upload transaction",
        )))
    }

    fn resolve_existing(&self, relative: &Path) -> Result<PathBuf, StorageError> {
        let mut candidate = self.root.clone();
        for component in relative.components() {
            candidate.push(component.as_os_str());
            if fs::symlink_metadata(&candidate)
                .map_err(map_existing_error)?
                .file_type()
                .is_symlink()
            {
                return Err(StorageError::AccessDenied);
            }
        }
        let canonical = fs::canonicalize(candidate).map_err(map_existing_error)?;
        if !canonical.starts_with(&self.root) {
            return Err(StorageError::AccessDenied);
        }
        Ok(canonical)
    }
}

pub struct PendingUpload {
    destination: PathBuf,
    temp_path: Option<PathBuf>,
    file: Option<tokio::fs::File>,
    bytes_written: u64,
    limit_bytes: u64,
}

impl PendingUpload {
    pub async fn write_chunk(&mut self, chunk: &[u8]) -> Result<(), StorageError> {
        let chunk_len = u64::try_from(chunk.len()).map_err(|_| StorageError::UploadTooLarge {
            limit_bytes: self.limit_bytes,
        })?;
        let next_size =
            self.bytes_written
                .checked_add(chunk_len)
                .ok_or(StorageError::UploadTooLarge {
                    limit_bytes: self.limit_bytes,
                })?;
        if next_size > self.limit_bytes {
            return Err(StorageError::UploadTooLarge {
                limit_bytes: self.limit_bytes,
            });
        }

        self.file
            .as_mut()
            .expect("active upload must own its temporary file")
            .write_all(chunk)
            .await
            .map_err(StorageError::Io)?;
        self.bytes_written = next_size;
        Ok(())
    }

    pub async fn commit(mut self) -> Result<u64, StorageError> {
        let mut file = self
            .file
            .take()
            .expect("active upload must own its temporary file");
        file.flush().await.map_err(StorageError::Io)?;
        file.sync_all().await.map_err(StorageError::Io)?;
        drop(file);

        let temp_path = self
            .temp_path
            .as_ref()
            .expect("active upload must own its temporary path");
        tokio::fs::hard_link(temp_path, &self.destination)
            .await
            .map_err(|error| {
                if error.kind() == io::ErrorKind::AlreadyExists {
                    StorageError::AlreadyExists
                } else {
                    StorageError::Io(error)
                }
            })?;
        tokio::fs::remove_file(temp_path)
            .await
            .map_err(StorageError::Io)?;
        self.temp_path = None;
        Ok(self.bytes_written)
    }
}

impl Drop for PendingUpload {
    fn drop(&mut self) {
        drop(self.file.take());
        if let Some(path) = self.temp_path.take() {
            let _ = fs::remove_file(path);
        }
    }
}

fn validated_relative_path(input: &str, allow_empty: bool) -> Result<PathBuf, StorageError> {
    if input.contains(['\0', '\\']) {
        return Err(StorageError::InvalidPath);
    }
    let virtual_relative = input.trim_start_matches('/');
    let path = Path::new(virtual_relative);
    let mut validated = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => validated.push(value),
            Component::CurDir
            | Component::ParentDir
            | Component::RootDir
            | Component::Prefix(_) => return Err(StorageError::InvalidPath),
        }
    }
    if validated.as_os_str().is_empty() && !allow_empty {
        return Err(StorageError::InvalidPath);
    }
    Ok(validated)
}

fn require_extension(path: &Path, allowed_extensions: &[&str]) -> Result<(), StorageError> {
    let extension = path
        .extension()
        .and_then(OsStr::to_str)
        .ok_or(StorageError::UnsupportedMediaType)?;
    if allowed_extensions
        .iter()
        .any(|allowed| extension.eq_ignore_ascii_case(allowed))
    {
        Ok(())
    } else {
        Err(StorageError::UnsupportedMediaType)
    }
}

fn display_virtual_path(relative: &Path) -> String {
    if relative.as_os_str().is_empty() {
        "/".to_owned()
    } else {
        format!("/{}", relative.to_string_lossy())
    }
}

fn map_existing_error(error: io::Error) -> StorageError {
    match error.kind() {
        io::ErrorKind::NotFound => StorageError::NotFound,
        io::ErrorKind::PermissionDenied => StorageError::AccessDenied,
        _ => StorageError::Io(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory {
        base: PathBuf,
        media: PathBuf,
    }

    impl TestDirectory {
        fn new() -> Self {
            let sequence = TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock must follow Unix epoch")
                .as_nanos();
            let base = std::env::temp_dir().join(format!(
                "rumiga-storage-test-{}-{timestamp}-{sequence}",
                std::process::id()
            ));
            let media = base.join("media");
            fs::create_dir_all(&media).expect("test storage root should be created");
            Self { base, media }
        }

        fn store(&self, limit: u64) -> MediaStore {
            MediaStore::new(&self.media, limit).expect("test media store should initialize")
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.base);
        }
    }

    #[test]
    fn list_is_sorted_and_reports_real_capacity() {
        let directory = TestDirectory::new();
        fs::create_dir(directory.media.join("Disks")).unwrap();
        fs::write(directory.media.join("z.adf"), [1, 2, 3]).unwrap();
        fs::write(directory.media.join("A.rom"), [4, 5]).unwrap();
        let store = directory.store(1024);

        let listing = store.list("/").unwrap();

        assert_eq!(listing.path, "/");
        assert_eq!(
            listing
                .files
                .iter()
                .map(|entry| (entry.name.as_str(), entry.size, entry.is_directory))
                .collect::<Vec<_>>(),
            [("Disks", 0, true), ("A.rom", 2, false), ("z.adf", 3, false)]
        );
        assert!(listing.total_bytes > 0);
        assert!(listing.free_bytes <= listing.total_bytes);
    }

    #[test]
    fn traversal_and_absolute_host_paths_are_rejected() {
        let directory = TestDirectory::new();
        let store = directory.store(1024);

        assert!(matches!(
            store.list("../outside"),
            Err(StorageError::InvalidPath)
        ));
        assert!(matches!(
            store.resolve_file("/../../etc/passwd", &["adf"]),
            Err(StorageError::InvalidPath)
        ));
        assert!(matches!(
            store.resolve_file("", &["adf"]),
            Err(StorageError::InvalidPath)
        ));
    }

    #[cfg(unix)]
    #[test]
    fn symlink_escape_is_rejected_and_hidden_from_listing() {
        use std::os::unix::fs::symlink;

        let directory = TestDirectory::new();
        let outside = directory.base.join("outside.adf");
        fs::write(&outside, [1, 2, 3]).unwrap();
        symlink(&outside, directory.media.join("escape.adf")).unwrap();
        let inside = directory.media.join("inside.adf");
        fs::write(&inside, [4, 5, 6]).unwrap();
        symlink(&inside, directory.media.join("alias.adf")).unwrap();
        let store = directory.store(1024);

        assert!(matches!(
            store.resolve_file("escape.adf", &["adf"]),
            Err(StorageError::AccessDenied)
        ));
        assert!(matches!(
            store.delete_file("alias.adf"),
            Err(StorageError::AccessDenied)
        ));
        assert_eq!(fs::read(inside).unwrap(), [4, 5, 6]);
        assert!(
            store
                .list("/")
                .unwrap()
                .files
                .iter()
                .all(|entry| entry.name != "escape.adf" && entry.name != "alias.adf")
        );
    }

    #[tokio::test]
    async fn upload_is_bounded_atomic_and_non_overwriting() {
        let directory = TestDirectory::new();
        let store = directory.store(5);
        let mut upload = store.begin_upload("Workbench.ADF").await.unwrap();
        upload.write_chunk(&[1, 2]).await.unwrap();
        upload.write_chunk(&[3, 4, 5]).await.unwrap();

        assert_eq!(upload.commit().await.unwrap(), 5);
        assert_eq!(
            fs::read(directory.media.join("Workbench.ADF")).unwrap(),
            [1, 2, 3, 4, 5]
        );
        assert!(matches!(
            store.begin_upload("Workbench.ADF").await,
            Err(StorageError::AlreadyExists)
        ));
        assert!(fs::read_dir(&directory.media).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(UPLOAD_TEMP_PREFIX)
        }));
    }

    #[tokio::test]
    async fn oversized_or_invalid_upload_leaves_no_file() {
        let directory = TestDirectory::new();
        let store = directory.store(4);
        let mut upload = store.begin_upload("too-large.hdf").await.unwrap();

        assert!(matches!(
            upload.write_chunk(&[1, 2, 3, 4, 5]).await,
            Err(StorageError::UploadTooLarge { limit_bytes: 4 })
        ));
        drop(upload);
        assert!(!directory.media.join("too-large.hdf").exists());
        assert!(matches!(
            store.begin_upload("nested/disk.adf").await,
            Err(StorageError::InvalidPath)
        ));
        assert!(matches!(
            store.begin_upload("notes.txt").await,
            Err(StorageError::UnsupportedMediaType)
        ));
    }

    #[test]
    fn deletion_is_confined_to_regular_media_files() {
        let directory = TestDirectory::new();
        fs::write(directory.media.join("disk.adf"), [1]).unwrap();
        fs::create_dir(directory.media.join("folder")).unwrap();
        let store = directory.store(1024);

        store.delete_file("disk.adf").unwrap();
        assert!(!directory.media.join("disk.adf").exists());
        assert!(matches!(
            store.delete_file("folder"),
            Err(StorageError::UnsupportedMediaType)
        ));
        assert!(matches!(
            store.delete_file("../outside.adf"),
            Err(StorageError::InvalidPath)
        ));
    }
}
