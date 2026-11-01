use reqwest::Url;
use serde::{Deserialize, Serialize};

pub mod asset_index;
pub mod version;
pub mod version_manifest;

fn serialize_url<S: serde::Serializer>(url: &Url, ser: S) -> Result<S::Ok, S::Error> {
    ser.serialize_str(url.as_str())
}

fn deserialize_url<'de, D: serde::Deserializer<'de>>(de: D) -> Result<Url, D::Error> {
    struct Visitor;

    impl<'de> serde::de::Visitor<'de> for Visitor {
        type Value = Url;

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Url::parse(v).map_err(E::custom)
        }

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("Url")
        }
    }
    de.deserialize_str(Visitor)
}

fn deserialize_maybe_single_vec<'de, T: serde::Deserialize<'de>, D: serde::Deserializer<'de>>(
    de: D,
) -> Result<Vec<T>, D::Error> {
    MaybeSingleVec::deserialize(de).map(From::from)
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub(self) enum MaybeSingleVec<T> {
    Single(T),
    Multiple(Vec<T>),
}

impl<T> From<MaybeSingleVec<T>> for Vec<T> {
    fn from(value: MaybeSingleVec<T>) -> Self {
        match value {
            MaybeSingleVec::Single(x) => vec![x],
            MaybeSingleVec::Multiple(items) => items,
        }
    }
}

impl<T> From<Vec<T>> for MaybeSingleVec<T> {
    fn from(value: Vec<T>) -> Self {
        if value.len() == 1 {
            unsafe {
                Self::Single(std::mem::transmute_prefix(Box::into_inner(
                    value.into_array::<1>().unwrap_unchecked(),
                )))
            }
        } else {
            Self::Multiple(value)
        }
    }
}
