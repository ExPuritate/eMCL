use std::{collections::HashMap, path::PathBuf, sync::Arc};

use getset::Getters;
use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};
use sha1::Digest;

use crate::{configuration, environment::path_to_asset_object, error::Error, sha1_serde::Sha1};

#[derive(Debug, Clone, Getters, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[getset(get = "pub")]
pub struct AssetIndex {
    objects: HashMap<String, AssetObject>,
}

impl AssetIndex {
    pub fn blocking_async_download(&self) -> Result<(), Error> {
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

        for (_path, obj) in self.objects.iter() {
            let client = Arc::clone(&client);
            let obj = *obj;
            if std::fs::read(obj.get_path())
                .is_ok_and(|contents| obj.hash == sha1::Sha1::digest(contents))
            {
                continue;
            }
            handles.push(rt.spawn(async move {
                let request = client.get(obj.get_url()).build()?;
                let response = client.execute(request.try_clone().unwrap()).await?;
                let mut contents = response.bytes().await?;
                while obj.hash != sha1::Sha1::digest(&contents) {
                    let response = client.execute(request.try_clone().unwrap()).await?;
                    contents = response.bytes().await?;
                }
                let mut file = tokio::fs::File::create(obj.get_path()).await?;
                tokio::io::AsyncWriteExt::write_all(&mut file, &contents).await?;
                Ok(())
            }));
        }

        for handle in handles {
            rt.block_on(handle)??;
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Getters, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[getset(get = "pub")]
pub struct AssetObject {
    hash: Sha1,
    size: usize,
}

impl AssetObject {
    pub fn get_url(&self) -> Url {
        let hash_str = self.hash.to_ascii_chars();
        Url::parse(&format!(
            "https://resources.download.minecraft.net/{}/{}",
            hash_str[0..2].as_str(),
            hash_str.as_str()
        ))
        .unwrap()
    }
    pub fn get_path(&self) -> PathBuf {
        path_to_asset_object(self.hash.to_ascii_chars().as_str())
    }
}

#[cfg(test)]
mod tests {
    use crate::manifest::version_manifest;

    #[test]
    fn download_latest_assets() {
        let version_manifest = version_manifest::get().unwrap();
        let latest_manifest = version_manifest
            .get_and_store(version_manifest.latest().release())
            .unwrap();
        let asset_index = latest_manifest.get_and_store_asset_index().unwrap();
        asset_index.blocking_async_download().unwrap();
    }
}
