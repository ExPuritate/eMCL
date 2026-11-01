use std::fmt::Display;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    ChecksumFailed,
    VersionNotFound,
    Jvm(#[from] jni::vm::JvmError),
    StartJvm(#[from] jni::errors::StartJvmError),
    Join(#[from] tokio::task::JoinError),
    IO(#[from] std::io::Error),
    Network(#[from] reqwest::Error),
    Json(#[from] serde_json::Error),
    ParseTemplate(#[from] subst::error::ParseError),
    ExpandTemplate(#[from] subst::error::ExpandError),
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        <_ as std::fmt::Debug>::fmt(self, f)
    }
}
