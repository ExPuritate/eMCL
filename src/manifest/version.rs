use std::{collections::HashMap, fmt::Display, path::PathBuf, sync::Arc};

use getset::Getters;
use iso8601_timestamp::Timestamp;
use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};
use sha1::Digest;

use crate::{
    configuration,
    environment::{assets_root, dir_to_asset_index, dir_to_version, path_to_library},
    error::Error,
    sha1_serde::Sha1,
};

#[derive(Debug, Clone, Getters, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[getset(get = "pub")]
pub struct ClientManifest {
    arguments: ClientArguments,
    asset_index: AssetIndex,
    assets: String,
    compliance_level: u8,
    downloads: Downloads,
    id: String,
    java_version: JavaVersion,
    libraries: Vec<LibraryElement>,
    logging: Logging,
    main_class: String,
    minimum_launcher_version: i32,
    release_time: Timestamp,
    time: Timestamp,
    #[serde(rename = "type")]
    r#type: VersionType,
}

impl ClientManifest {
    pub fn get_asset_index(&self) -> Result<super::asset_index::AssetIndex, Error> {
        let data = reqwest::blocking::get(self.asset_index.url.clone())?.bytes()?;
        if self.asset_index.sha1 != sha1::Sha1::digest(&data) {
            return Err(Error::ChecksumFailed.into());
        }
        serde_json::from_slice(&data).map_err(From::from)
    }
    pub fn get_and_store_asset_index(&self) -> Result<super::asset_index::AssetIndex, Error> {
        let data = reqwest::blocking::get(self.asset_index.url.clone())?.bytes()?;
        if self.asset_index.sha1 != sha1::Sha1::digest(&data) {
            return Err(Error::ChecksumFailed.into());
        }
        std::fs::write(
            dir_to_asset_index().join(format!("{}.json", self.assets)),
            &data,
        )?;
        serde_json::from_slice(&data).map_err(From::from)
    }
    pub fn store_client_jar(&self) -> Result<(), Error> {
        let data = reqwest::blocking::get(self.downloads.client.url.clone())?.bytes()?;
        if self.downloads.client.sha1 != sha1::Sha1::digest(&data) {
            return Err(Error::ChecksumFailed.into());
        }
        let path = dir_to_version(&self.id).join(format!("{}.jar", self.id));
        if std::fs::read(&path).is_ok_and(|x| self.downloads.client.sha1 == sha1::Sha1::digest(x)) {
            return Ok(());
        }
        std::fs::write(
            dir_to_version(&self.id).join(format!("{}.jar", self.id)),
            &data,
        )?;
        Ok(())
    }
    pub fn blocking_async_download_libraries(&self) -> Result<(), Error> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .worker_threads(configuration::max_threads())
            .enable_time()
            .build()?;

        let mut handles: Vec<tokio::task::JoinHandle<Result<(), Error>>> = Vec::with_capacity(16);
        let client = Arc::new(
            Client::builder()
                .retry(reqwest::retry::never().max_retries_per_request(10))
                .build()?,
        );

        for library in &self.libraries {
            if !library.rules.iter().all(Rule::is_allowed) {
                #[cfg(test)]
                eprintln!("Skipping {}", library.name);
                continue;
            }
            let path = path_to_library(&library.downloads.artifact.path);
            let sha1 = library.downloads.artifact.sha1;
            let url = library.downloads.artifact.url.clone();
            if std::fs::read(&path).is_ok_and(|x| sha1 == sha1::Sha1::digest(x)) {
                return Ok(());
            }
            let client = Arc::clone(&client);
            handles.push(rt.spawn(async move {
                let request = client.get(url).build()?;
                let response = client.execute(request.try_clone().unwrap()).await?;
                let mut contents = response.bytes().await?;
                while sha1 != sha1::Sha1::digest(&contents) {
                    let response = client.execute(request.try_clone().unwrap()).await?;
                    contents = response.bytes().await?;
                }
                let mut file = tokio::fs::File::create(path).await?;
                tokio::io::AsyncWriteExt::write_all(&mut file, &contents).await?;
                Ok(())
            }));
        }

        for handle in handles {
            rt.block_on(handle)??;
        }

        Ok(())
    }
    pub fn get_class_path(&self) -> Vec<String> {
        let mut result: Vec<String> = self
            .libraries
            .iter()
            .filter(|library| library.rules.iter().all(Rule::is_allowed))
            .map(|x| {
                path_to_library(&x.downloads.artifact.path)
                    .display()
                    .to_string()
            })
            .collect();
        result.push(
            dir_to_version(&self.id)
                .join(format!("{}.jar", self.id))
                .display()
                .to_string(),
        );
        result
    }
    pub fn store_log_config(&self) -> Result<(), Error> {
        let contents = reqwest::blocking::get(self.logging.client.file.url.clone())?.bytes()?;
        if self.logging.client.file.sha1 != sha1::Sha1::digest(&contents) {
            return Err(Error::ChecksumFailed);
        }
        std::fs::write(self.path_to_log_config(), contents)?;
        Ok(())
    }
    pub fn path_to_log_config(&self) -> PathBuf {
        let mut result = assets_root().join("log_configs");
        std::fs::create_dir_all(&result).unwrap();
        result.push(&self.logging.client.file.id);
        result
    }
}

#[derive(Debug, Clone, Getters, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[getset(get = "pub")]
pub struct ClientArguments {
    default_user_jvm: Vec<DefaultUserJvmArguments>,
    game: Vec<GameArguments>,
    jvm: Vec<JvmArguments>,
}

#[derive(Debug, Clone, Getters, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[getset(get = "pub")]
pub struct DefaultUserJvmArguments {
    #[serde(default)]
    rules: Vec<Rule>,
    #[serde(deserialize_with = "super::deserialize_maybe_single_vec")]
    value: Vec<String>,
}

impl DefaultUserJvmArguments {
    pub fn is_allowed(&self) -> bool {
        self.rules.iter().all(Rule::is_allowed)
    }
}

/* #region game */
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum GameArguments {
    Value(String),
    WithRules {
        rules: Vec<GameArgumentRule>,
        #[serde(deserialize_with = "super::deserialize_maybe_single_vec")]
        value: Vec<String>,
    },
}

impl GameArguments {
    pub fn is_allowed(
        &self,
        is_demo_user: bool,
        has_custom_resolution: bool,
        has_quick_plays_support: bool,
        is_quick_play_singleplayer: bool,
        is_quick_play_multiplayer: bool,
        is_quick_play_realms: bool,
    ) -> bool {
        match self {
            GameArguments::Value(_) => true,
            GameArguments::WithRules { rules, value: _ } => rules.iter().all(|x| {
                x.is_allowed(
                    is_demo_user,
                    has_custom_resolution,
                    has_quick_plays_support,
                    is_quick_play_singleplayer,
                    is_quick_play_multiplayer,
                    is_quick_play_realms,
                )
            }),
        }
    }
}

#[derive(Debug, Clone, Getters, Serialize, Deserialize)]
#[getset(get = "pub")]
pub struct GameArgumentRule {
    action: RuleAction,
    features: HashMap<GameArgumentRuleFeature, bool>,
}

impl GameArgumentRule {
    pub fn is_allowed(
        &self,
        is_demo_user: bool,
        has_custom_resolution: bool,
        has_quick_plays_support: bool,
        is_quick_play_singleplayer: bool,
        is_quick_play_multiplayer: bool,
        is_quick_play_realms: bool,
    ) -> bool {
        let result = self.features.iter().all(|(key, val)| match key {
            GameArgumentRuleFeature::IsDemoUser => is_demo_user.eq(val),
            GameArgumentRuleFeature::HasCustomResolution => has_custom_resolution.eq(val),
            GameArgumentRuleFeature::HasQuickPlaysSupport => has_quick_plays_support.eq(val),
            GameArgumentRuleFeature::IsQuickPlaySingleplayer => is_quick_play_singleplayer.eq(val),
            GameArgumentRuleFeature::IsQuickPlayMultiplayer => is_quick_play_multiplayer.eq(val),
            GameArgumentRuleFeature::IsQuickPlayRealms => is_quick_play_realms.eq(val),
        });

        match self.action {
            RuleAction::Allow => result,
            RuleAction::Disallow => !result,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum GameArgumentRuleFeature {
    IsDemoUser,
    HasCustomResolution,
    HasQuickPlaysSupport,
    IsQuickPlaySingleplayer,
    IsQuickPlayMultiplayer,
    IsQuickPlayRealms,
}
/* #endregion */

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JvmArguments {
    Value(String),
    WithRules {
        rules: Vec<Rule>,
        #[serde(deserialize_with = "super::deserialize_maybe_single_vec")]
        value: Vec<String>,
    },
}

impl JvmArguments {
    pub fn is_allowed(&self) -> bool {
        match self {
            Self::Value(_) => true,
            Self::WithRules { rules, value: _ } => rules.iter().all(Rule::is_allowed),
        }
    }
}

#[derive(Debug, Clone, Getters, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[getset(get = "pub")]
pub struct Rule {
    action: RuleAction,
    os: RuleOs,
}

impl Rule {
    pub fn is_allowed(&self) -> bool {
        match self.action {
            RuleAction::Allow => self.os.to_bool(),
            RuleAction::Disallow => !self.os.to_bool(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RuleAction {
    Allow,
    Disallow,
}

#[derive(Debug, Clone, Getters, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[getset(get = "pub")]
pub struct RuleOs {
    name: Option<String>,
    version: Option<String>,
    version_range: Option<VersionRange>,
    arch: Option<String>,
}

impl RuleOs {
    #[cfg(windows)]
    fn parse_version(ver: &str) -> windows_version::OsVersion {
        let mut iter = ver.split('.');
        let Some(Ok(major)) = iter.next().map(str::parse) else {
            return windows_version::OsVersion::current();
        };
        let Some(Ok(minor)) = iter.next().map(str::parse) else {
            return windows_version::OsVersion::current();
        };
        let Some(Ok(build)) = iter.next().map(str::parse) else {
            return windows_version::OsVersion::current();
        };
        windows_version::OsVersion::new(major, minor, 0, build)
    }
    pub fn to_bool(&self) -> bool {
        self.name.as_deref().is_none_or(|x| match x {
            "osx" => cfg!(target_os = "macos"),
            "linux" => cfg!(target_os = "linux"),
            "windows" => cfg!(target_os = "windows"),
            _ => false,
        }) && self.version.as_deref().is_none_or(|x| {
            #[cfg(windows)]
            {
                Self::parse_version(x) == windows_version::OsVersion::current()
            }
        }) && self.version_range.as_ref().is_none_or(|x| {
            #[cfg(windows)]
            {
                let current = windows_version::OsVersion::current();
                let Some(max) = x.max.as_deref() else {
                    return true;
                };
                let result = Self::parse_version(max) > current;
                let Some(min) = x.min.as_deref() else {
                    return result;
                };
                result && Self::parse_version(min) < current
            }
        })
    }
}

#[derive(Debug, Clone, Getters, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[getset(get = "pub")]
pub struct VersionRange {
    max: Option<String>,
    min: Option<String>,
}

#[derive(Debug, Clone, Getters, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[getset(get = "pub")]
pub struct AssetIndex {
    id: String,
    sha1: Sha1,
    size: usize,
    total_size: usize,
    #[serde(
        serialize_with = "super::serialize_url",
        deserialize_with = "super::deserialize_url"
    )]
    url: Url,
}

#[derive(Debug, Clone, Getters, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[getset(get = "pub")]
pub struct Downloads {
    client: DownloadCompound,
    server: DownloadCompound,
}

#[derive(Debug, Clone, Getters, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[getset(get = "pub")]
pub struct DownloadCompound {
    sha1: Sha1,
    size: usize,
    #[serde(
        serialize_with = "super::serialize_url",
        deserialize_with = "super::deserialize_url"
    )]
    url: Url,
}

#[derive(Debug, Clone, Getters, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[getset(get = "pub")]
pub struct JavaVersion {
    component: String,
    major_version: i32,
}

/* #region libraries */
#[derive(Debug, Clone, Getters, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[getset(get = "pub")]
pub struct LibraryElement {
    downloads: LibraryDownloads,
    name: String,
    #[serde(default)]
    rules: Vec<Rule>,
}

#[derive(Debug, Clone, Getters, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[getset(get = "pub")]
pub struct LibraryDownloads {
    artifact: LibraryDownloadsArtifact,
}

#[derive(Debug, Clone, Getters, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[getset(get = "pub")]
pub struct LibraryDownloadsArtifact {
    path: String,
    sha1: Sha1,
    size: usize,
    #[serde(
        serialize_with = "super::serialize_url",
        deserialize_with = "super::deserialize_url"
    )]
    url: Url,
}
/* #endregion */

/* #region logging */
#[derive(Debug, Clone, Getters, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[getset(get = "pub")]
pub struct Logging {
    client: LoggingClient,
}

#[derive(Debug, Clone, Getters, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[getset(get = "pub")]
pub struct LoggingClient {
    argument: String,
    file: LoggingClientFile,
    #[serde(rename = "type")]
    r#type: String,
}

#[derive(Debug, Clone, Getters, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[getset(get = "pub")]
pub struct LoggingClientFile {
    id: String,
    sha1: Sha1,
    size: usize,
    #[serde(
        serialize_with = "super::serialize_url",
        deserialize_with = "super::deserialize_url"
    )]
    url: Url,
}
/* #endregion */

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VersionType {
    Release,
    Snapshot,
    OldBeta,
    OldAlpha,
    #[serde(other)]
    Unknown,
}

impl Display for VersionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VersionType::Release => write!(f, "release"),
            VersionType::Snapshot => write!(f, "snapshot"),
            VersionType::OldBeta => write!(f, "old_beta"),
            VersionType::OldAlpha => write!(f, "old_alpha"),
            VersionType::Unknown => write!(f, "unknown"),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::manifest::version_manifest;

    #[test]
    fn download_latest_client() {
        let version_manifest = version_manifest::get().unwrap();
        let latest_manifest = version_manifest
            .get_and_store(version_manifest.latest().release())
            .unwrap();
        latest_manifest.store_client_jar().unwrap();
        latest_manifest.blocking_async_download_libraries().unwrap();
        latest_manifest.store_log_config().unwrap();
    }
}
