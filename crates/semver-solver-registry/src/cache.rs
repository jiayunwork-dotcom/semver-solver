use std::path::{Path, PathBuf};
use std::time::Duration;
use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};
use semver_solver_core::{PackageName, error::Result};
use crate::registry::PackageInfo;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheEntry {
    data: PackageInfo,
    cached_at: DateTime<Utc>,
    ttl_seconds: u64,
}

pub struct RegistryCache {
    cache_dir: PathBuf,
    default_ttl: Duration,
}

impl RegistryCache {
    pub fn new(cache_dir: PathBuf, default_ttl: Duration) -> Result<Self> {
        std::fs::create_dir_all(&cache_dir)?;
        Ok(Self {
            cache_dir,
            default_ttl,
        })
    }

    pub fn new_default() -> Result<Self> {
        let cache_dir = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".semver-cache");
        Self::new(cache_dir, Duration::from_secs(24 * 60 * 60))
    }

    fn cache_path(&self, pm: &str, name: &PackageName) -> PathBuf {
        let safe_name = name.as_str()
            .replace('/', "_")
            .replace('\\', "_")
            .replace(':', "_");
        self.cache_dir.join(pm).join(format!("{}.json", safe_name))
    }

    pub fn get(&self, pm: &str, name: &PackageName) -> Option<PackageInfo> {
        let path = self.cache_path(pm, name);
        if !path.exists() {
            return None;
        }

        let content = std::fs::read_to_string(&path).ok()?;
        let entry: CacheEntry = serde_json::from_str(&content).ok()?;

        let elapsed = Utc::now().signed_duration_since(entry.cached_at);
        if elapsed.num_seconds() as u64 > entry.ttl_seconds {
            let _ = std::fs::remove_file(&path);
            return None;
        }

        Some(entry.data)
    }

    pub fn set(&self, pm: &str, name: &PackageName, data: &PackageInfo) -> Result<()> {
        self.set_with_ttl(pm, name, data, self.default_ttl)
    }

    pub fn set_with_ttl(&self, pm: &str, name: &PackageName, data: &PackageInfo, ttl: Duration) -> Result<()> {
        let path = self.cache_path(pm, name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let entry = CacheEntry {
            data: data.clone(),
            cached_at: Utc::now(),
            ttl_seconds: ttl.as_secs(),
        };

        let content = serde_json::to_string_pretty(&entry)?;
        std::fs::write(&path, content)?;
        Ok(())
    }

    pub fn invalidate(&self, pm: &str, name: &PackageName) -> Result<()> {
        let path = self.cache_path(pm, name);
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        Ok(())
    }

    pub fn clear(&self) -> Result<()> {
        if self.cache_dir.exists() {
            std::fs::remove_dir_all(&self.cache_dir)?;
            std::fs::create_dir_all(&self.cache_dir)?;
        }
        Ok(())
    }

    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }
}
