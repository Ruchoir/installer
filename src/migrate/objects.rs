//! The objects: read from the copy of the monolith's store, written to the drive's.
//!
//! The copy is a directory where each object sits at its key (`spaces/<space>/<file>/<version>`,
//! as `rclone copy` or `aws s3 sync` of the monolith's bucket leave it). The drive's store is its
//! Garage bucket, reached over S3 on the loopback while the migration runs ([`super::instance`]),
//! or a directory laid out the same way in the tests.
//!
//! A version keeps its object key: the drive reads the key from the version's row and never
//! rebuilds it, so `spaces/...` keys stay valid beside the drive's own `organisations/...` ones.

use std::path::{Path, PathBuf};

use s3::bucket::Bucket;
use s3::creds::Credentials;
use s3::error::S3Error;
use s3::region::Region;

/// The copy of the monolith's object store.
pub struct Objects {
    root: PathBuf,
}

impl Objects {
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_owned(),
        }
    }

    /// The object at `key`, `None` when the copy does not hold it.
    pub fn get(&self, key: &str) -> Option<Vec<u8>> {
        // Keys are the monolith's own, never a name someone typed; still, nothing outside the copy.
        if key.split('/').any(|part| part.is_empty() || part == "..") {
            return None;
        }
        std::fs::read(self.root.join(key)).ok()
    }
}

/// The drive's object store.
pub enum Store {
    S3(Box<Bucket>),
    Directory(PathBuf),
}

impl Store {
    /// The drive's bucket at `endpoint` (`http://127.0.0.1:<port>`), with the drive's key.
    pub fn s3(endpoint: &str, bucket: &str, access: &str, secret: &str) -> Result<Self, String> {
        let credentials = Credentials::new(Some(access), Some(secret), None, None, None)
            .map_err(|e| e.to_string())?;
        let region = Region::Custom {
            region: "garage".to_owned(),
            endpoint: endpoint.to_owned(),
        };
        let bucket = Bucket::new(bucket, region, credentials)
            .map_err(|e| e.to_string())?
            .with_path_style();
        Ok(Self::S3(bucket))
    }

    /// Whether `key` is already there.
    pub async fn has(&self, key: &str) -> Result<bool, String> {
        match self {
            Self::S3(bucket) => match bucket.head_object(key).await {
                Ok(_) => Ok(true),
                Err(S3Error::HttpFailWithBody(404, _)) => Ok(false),
                Err(error) => Err(format!("the drive's store: {error}")),
            },
            Self::Directory(root) => Ok(root.join(key).exists()),
        }
    }

    pub async fn put(&self, key: &str, bytes: &[u8]) -> Result<(), String> {
        match self {
            Self::S3(bucket) => bucket
                .put_object(key, bytes)
                .await
                .map(|_| ())
                .map_err(|e| format!("the drive's store: {e}")),
            Self::Directory(root) => {
                let path = root.join(key);
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                std::fs::write(path, bytes).map_err(|e| e.to_string())
            }
        }
    }
}
