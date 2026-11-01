use std::sync::OnceLock;

use getset::Getters;
use iso8601_timestamp::Timestamp;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use sha1::Digest;

use crate::{
    configuration,
    environment::dir_to_version,
    error::Error,
    manifest::version::{ClientManifest, VersionType},
    sha1_serde::Sha1,
};

#[derive(Debug, Clone, Getters, Serialize, Deserialize)]
#[getset(get = "pub")]
pub struct VersionManifest {
    latest: VersionManifestLatest,
    versions: Vec<VersionManifestElement>,
}

impl VersionManifest {
    pub fn get_client(&self, id: &str) -> Result<ClientManifest, Error> {
        let Some(info) = self.versions.iter().find(|x| x.id.eq(id)) else {
            return Err(Error::VersionNotFound);
        };
        let data = reqwest::blocking::get(info.url.clone())?.bytes()?;
        if info.sha1 != sha1::Sha1::digest(&data) {
            return Err(Error::ChecksumFailed.into());
        }
        serde_json::from_slice(&data).map_err(From::from)
    }
    pub fn get_and_store(&self, id: &str) -> Result<ClientManifest, Error> {
        let Some(info) = self.versions.iter().find(|x| x.id.eq(id)) else {
            return Err(Error::VersionNotFound);
        };
        if let Ok(file) =
            std::fs::File::open_buffered(dir_to_version(&info.id).join(format!("{}.json", info.id)))
            && let Ok(result) = serde_json::from_reader::<_, ClientManifest>(file)
        {
            return Ok(result);
        }
        let data = reqwest::blocking::get(info.url.clone())?.bytes()?;
        if info.sha1 != sha1::Sha1::digest(&data) {
            return Err(Error::ChecksumFailed.into());
        }
        std::fs::write(
            dir_to_version(&info.id).join(format!("{}.json", info.id)),
            &data,
        )?;
        serde_json::from_slice(&data).map_err(From::from)
    }
    pub fn get_latest_release(&self) -> Result<ClientManifest, Error> {
        self.get_client(self.latest.release())
    }
    pub fn get_latest_snapshot(&self) -> Result<ClientManifest, Error> {
        self.get_client(self.latest.snapshot())
    }
}

#[derive(Debug, Clone, Getters, Serialize, Deserialize)]
#[getset(get = "pub")]
pub struct VersionManifestLatest {
    release: String,
    snapshot: String,
}

#[derive(Debug, Clone, Getters, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[getset(get = "pub")]
pub struct VersionManifestElement {
    id: String,
    #[serde(rename = "type")]
    r#type: VersionType,
    #[serde(
        serialize_with = "super::serialize_url",
        deserialize_with = "super::deserialize_url"
    )]
    url: Url,
    time: Timestamp,
    release_time: Timestamp,
    sha1: Sha1,
    compliance_level: u8,
}

pub fn get() -> Result<&'static VersionManifest, anyhow::Error> {
    static DATA: OnceLock<VersionManifest> = OnceLock::new();
    DATA.get_or_try_init(|| {
        serde_json::from_reader(reqwest::blocking::get(
            configuration::get()
                .read()
                .version_manifest_version()
                .get_url()
                .clone(),
        )?)
        .map_err(From::from)
    })
}
