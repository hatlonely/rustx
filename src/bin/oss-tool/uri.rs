// URI parsing for oss-tool

use anyhow::{anyhow, Result};

/// Supported cloud providers
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Provider {
    /// Amazon S3
    S3,
    /// Aliyun OSS
    Oss,
    /// Google Cloud Storage
    Gcs,
}

impl Provider {
    /// Parse provider from scheme string
    pub fn from_scheme(scheme: &str) -> Result<Self> {
        match scheme.to_lowercase().as_str() {
            "s3" => Ok(Provider::S3),
            "oss" => Ok(Provider::Oss),
            "gcs" | "gs" => Ok(Provider::Gcs),
            _ => Err(anyhow!("Unknown provider scheme: {}", scheme)),
        }
    }

    /// Get scheme string for the provider
    pub fn scheme(&self) -> &'static str {
        match self {
            Provider::S3 => "s3",
            Provider::Oss => "oss",
            Provider::Gcs => "gcs",
        }
    }
}

/// Parsed OSS URI structure
#[derive(Debug, Clone)]
pub struct OssUri {
    /// Cloud provider (s3, oss, gcs)
    pub provider: Provider,
    /// Bucket name
    pub bucket: String,
    /// Object key (path within bucket)
    pub key: String,
}

impl OssUri {
    /// Parse a URI string into OssUri
    ///
    /// Supported formats:
    /// - s3://bucket/path/to/key
    /// - oss://bucket/path/to/key
    /// - gcs://bucket/path/to/key
    /// - gs://bucket/path/to/key
    pub fn parse(uri: &str) -> Result<Self> {
        // Check for scheme separator
        let parts: Vec<&str> = uri.splitn(2, "://").collect();
        if parts.len() != 2 {
            return Err(anyhow!(
                "Invalid URI format. Expected: scheme://bucket/key, got: {}",
                uri
            ));
        }

        let scheme = parts[0];
        let provider = Provider::from_scheme(scheme)?;

        // Parse bucket and key
        let path = parts[1];
        if path.is_empty() {
            return Err(anyhow!("Bucket name is required in URI: {}", uri));
        }

        let (bucket, key) = match path.find('/') {
            Some(idx) => {
                let bucket = &path[..idx];
                let key = &path[idx + 1..];
                (bucket.to_string(), key.to_string())
            }
            None => (path.to_string(), String::new()),
        };

        if bucket.is_empty() {
            return Err(anyhow!("Bucket name cannot be empty in URI: {}", uri));
        }

        Ok(OssUri {
            provider,
            bucket,
            key,
        })
    }

    /// Check if the URI represents a directory (ends with '/')
    pub fn is_directory(&self) -> bool {
        self.key.is_empty() || self.key.ends_with('/')
    }

    /// Get the full URI string
    pub fn to_string(&self) -> String {
        if self.key.is_empty() {
            format!("{}://{}", self.provider.scheme(), self.bucket)
        } else {
            format!("{}://{}/{}", self.provider.scheme(), self.bucket, self.key)
        }
    }

    /// Get parent directory key
    pub fn parent_key(&self) -> Option<String> {
        if self.key.is_empty() {
            return None;
        }

        let key = self.key.trim_end_matches('/');
        match key.rfind('/') {
            Some(idx) => Some(format!("{}/", &key[..idx])),
            None => Some(String::new()),
        }
    }

    /// Get the file name part of the key
    pub fn file_name(&self) -> Option<&str> {
        if self.key.is_empty() {
            return None;
        }

        let key = self.key.trim_end_matches('/');
        match key.rfind('/') {
            Some(idx) => Some(&key[idx + 1..]),
            None => Some(key),
        }
    }
}

/// Check if a path is a remote URI
pub fn is_remote_uri(path: &str) -> bool {
    path.contains("://")
}

/// Represents either a local path or a remote URI
#[derive(Debug, Clone)]
pub enum Location {
    Local(String),
    Remote(OssUri),
}

impl Location {
    /// Parse a path string into a Location
    pub fn parse(path: &str) -> Result<Self> {
        if is_remote_uri(path) {
            Ok(Location::Remote(OssUri::parse(path)?))
        } else {
            Ok(Location::Local(path.to_string()))
        }
    }

    /// Check if this is a local path
    pub fn is_local(&self) -> bool {
        matches!(self, Location::Local(_))
    }

    /// Check if this is a remote URI
    pub fn is_remote(&self) -> bool {
        matches!(self, Location::Remote(_))
    }

    /// Get the local path if this is a local location
    pub fn as_local(&self) -> Option<&str> {
        match self {
            Location::Local(path) => Some(path),
            Location::Remote(_) => None,
        }
    }

    /// Get the remote URI if this is a remote location
    pub fn as_remote(&self) -> Option<&OssUri> {
        match self {
            Location::Local(_) => None,
            Location::Remote(uri) => Some(uri),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_s3_uri() {
        let uri = OssUri::parse("s3://my-bucket/path/to/file.txt").unwrap();
        assert_eq!(uri.provider, Provider::S3);
        assert_eq!(uri.bucket, "my-bucket");
        assert_eq!(uri.key, "path/to/file.txt");
    }

    #[test]
    fn test_parse_oss_uri() {
        let uri = OssUri::parse("oss://my-bucket/path/to/file.txt").unwrap();
        assert_eq!(uri.provider, Provider::Oss);
        assert_eq!(uri.bucket, "my-bucket");
        assert_eq!(uri.key, "path/to/file.txt");
    }

    #[test]
    fn test_parse_gcs_uri() {
        let uri = OssUri::parse("gcs://my-bucket/path/to/file.txt").unwrap();
        assert_eq!(uri.provider, Provider::Gcs);
        assert_eq!(uri.bucket, "my-bucket");
        assert_eq!(uri.key, "path/to/file.txt");

        // Test gs:// alias
        let uri2 = OssUri::parse("gs://my-bucket/path/to/file.txt").unwrap();
        assert_eq!(uri2.provider, Provider::Gcs);
    }

    #[test]
    fn test_parse_uri_no_key() {
        let uri = OssUri::parse("s3://my-bucket").unwrap();
        assert_eq!(uri.bucket, "my-bucket");
        assert_eq!(uri.key, "");
        assert!(uri.is_directory());
    }

    #[test]
    fn test_parse_uri_with_trailing_slash() {
        let uri = OssUri::parse("s3://my-bucket/prefix/").unwrap();
        assert_eq!(uri.key, "prefix/");
        assert!(uri.is_directory());
    }

    #[test]
    fn test_parse_invalid_uri() {
        assert!(OssUri::parse("invalid").is_err());
        assert!(OssUri::parse("unknown://bucket/key").is_err());
        assert!(OssUri::parse("s3://").is_err());
    }

    #[test]
    fn test_is_remote_uri() {
        assert!(is_remote_uri("s3://bucket/key"));
        assert!(is_remote_uri("oss://bucket/key"));
        assert!(is_remote_uri("gcs://bucket/key"));
        assert!(!is_remote_uri("/local/path"));
        assert!(!is_remote_uri("./relative/path"));
        assert!(!is_remote_uri("file.txt"));
    }

    #[test]
    fn test_location_parse() {
        let local = Location::parse("/local/path").unwrap();
        assert!(local.is_local());
        assert_eq!(local.as_local(), Some("/local/path"));

        let remote = Location::parse("s3://bucket/key").unwrap();
        assert!(remote.is_remote());
        assert!(remote.as_remote().is_some());
    }

    #[test]
    fn test_file_name() {
        let uri = OssUri::parse("s3://bucket/path/to/file.txt").unwrap();
        assert_eq!(uri.file_name(), Some("file.txt"));

        let uri2 = OssUri::parse("s3://bucket/path/to/dir/").unwrap();
        assert_eq!(uri2.file_name(), Some("dir"));

        let uri3 = OssUri::parse("s3://bucket").unwrap();
        assert_eq!(uri3.file_name(), None);
    }

    #[test]
    fn test_parent_key() {
        let uri = OssUri::parse("s3://bucket/path/to/file.txt").unwrap();
        assert_eq!(uri.parent_key(), Some("path/to/".to_string()));

        let uri2 = OssUri::parse("s3://bucket/file.txt").unwrap();
        assert_eq!(uri2.parent_key(), Some("".to_string()));

        let uri3 = OssUri::parse("s3://bucket").unwrap();
        assert_eq!(uri3.parent_key(), None);
    }
}
