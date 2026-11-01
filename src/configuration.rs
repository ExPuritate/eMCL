use std::{
    fs::File,
    io::{Seek, Write},
    sync::{
        LazyLock, OnceLock,
        nonpoison::{MappedRwLockReadGuard, RwLock, RwLockReadGuard},
    },
};

use getset::Getters;
use reqwest::Url;
use serde::{Deserialize, Serialize};

use crate::environment::configuration_path;

#[derive(Debug, Clone, Getters, Serialize, Deserialize)]
#[getset(get = "pub")]
pub struct Configuration {
    #[serde(default)]
    version_manifest_version: VersionManifestVersion,
    #[serde(default = "Configuration::default_max_threads")]
    max_threads: usize,
    #[serde(default = "Configuration::default_launcher_name")]
    launcher_name: String,
    #[serde(default = "Configuration::default_launcher_version")]
    launcher_version: String,
}

impl Configuration {
    pub const fn default_max_threads() -> usize {
        4
    }
    pub fn default_launcher_name() -> String {
        "eMCL".to_owned()
    }
    pub fn default_launcher_version() -> String {
        env!("CARGO_PKG_VERSION").to_owned()
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub enum VersionManifestVersion {
    V1,
    #[default]
    V2,
}

impl VersionManifestVersion {
    pub fn get_url(&self) -> &'static Url {
        static V1: LazyLock<Url> = LazyLock::new(|| {
            Url::parse("https://piston-meta.mojang.com/mc/game/version_manifest.json").unwrap()
        });
        static V2: LazyLock<Url> = LazyLock::new(|| {
            Url::parse("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json").unwrap()
        });

        match self {
            VersionManifestVersion::V1 => &V1,
            VersionManifestVersion::V2 => &V2,
        }
    }
}

pub fn get() -> &'static RwLock<Configuration> {
    static CFG: OnceLock<RwLock<Configuration>> = OnceLock::new();

    CFG.get_or_init(|| {
        let path = configuration_path();
        let mut file = File::options()
            .read(true)
            .create(true)
            .append(true)
            .open(&path)
            .unwrap();
        if file.metadata().unwrap().len() == 0 {
            file.write_all(b"{}").unwrap();
        }
        file.seek(std::io::SeekFrom::Start(0)).unwrap();
        RwLock::new(serde_json::from_reader(&mut file).unwrap())
    })
}

pub fn max_threads() -> usize {
    *get().read().max_threads()
}

pub fn launcher_name() -> MappedRwLockReadGuard<'static, str> {
    RwLockReadGuard::map(get().read(), |x| &**x.launcher_name())
}

pub fn launcher_version() -> MappedRwLockReadGuard<'static, str> {
    RwLockReadGuard::map(get().read(), |x| &**x.launcher_version())
}
