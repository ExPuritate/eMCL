use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    thread::JoinHandle,
};

use jni::{
    JValue, jni_sig, jni_str,
    objects::{JObjectArray, JString},
    strings::JNIString,
};
use serde::{Deserialize, Serialize};
use subst::{Template, error::ExpandError};

use crate::{
    configuration::{launcher_name, launcher_version},
    environment::{assets_root, dir_to_version, game_directory},
    error::Error,
    manifest::version::{ClientManifest, JvmArguments},
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ResolutionSetting {
    resolution_width: usize,
    resolution_height: usize,
}

struct CdGuard(PathBuf);

impl CdGuard {
    pub fn new(dest: impl AsRef<Path>) -> Self {
        let pwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(dest).unwrap();
        Self(pwd)
    }
}

impl Drop for CdGuard {
    fn drop(&mut self) {
        std::env::set_current_dir(&self.0).unwrap();
    }
}

const PATH_SEPERATOR: &str = cfg_select! {
    windows => ";",
    unix => ":",
};

pub fn launch(
    version: &str,
    auth_player_name: &str,
    auth_uuid: Option<&str>,
    auth_access_token: &str,
    clientid: Option<&str>,
    auth_xuid: Option<&str>,
    is_demo: bool,
    custom_resolution: Option<ResolutionSetting>,
    disable_multiplayer: bool,
    disable_chat: bool,
    quick_play_path: Option<&str>,
    quick_play_singleplayer: Option<&str>,
    quick_play_multiplayer: Option<&str>,
    quick_play_realms: Option<&str>,
) -> Result<(JoinHandle<Result<(), jni::errors::Error>>, jni::vm::JavaVM), Error> {
    let _cd_guard = CdGuard::new(game_directory());
    let version_dir = dir_to_version(version);
    let manifest: ClientManifest = serde_json::from_reader(std::fs::File::open_buffered(
        version_dir.join(format!("{version}.json")),
    )?)?;

    let mut jvm_init_args = jni::vm::InitArgsBuilder::new();

    for default_user_jvm in manifest.arguments().default_user_jvm() {
        if !default_user_jvm.is_allowed() {
            continue;
        }
        for opt_string in default_user_jvm.value() {
            jvm_init_args = jvm_init_args.option(opt_string);
        }
    }

    let natives_directory = version_dir.join("natives");
    let jvm_map = HashMap::from([
        ("natives_directory", natives_directory.display().to_string()),
        ("launcher_name", launcher_name().to_owned()),
        ("launcher_version", launcher_version().to_owned()),
    ]);
    let mut reaches_classpath = false;
    let mut reaches_add_exports = false;
    for jvm in manifest.arguments().jvm() {
        if !jvm.is_allowed() {
            continue;
        }
        match jvm {
            JvmArguments::Value(opt_string) => {
                if "-cp".eq(opt_string) {
                    reaches_classpath = true;
                    continue;
                }
                if opt_string == "--add-exports" {
                    reaches_add_exports = true;
                    continue;
                }
                if reaches_classpath && opt_string == "${classpath}" {
                    jvm_init_args = jvm_init_args.option(format!(
                        "-Djava.class.path={}",
                        manifest.get_class_path().join(PATH_SEPERATOR)
                    ));
                    reaches_classpath = false;
                    continue;
                }
                if reaches_add_exports {
                    jvm_init_args = jvm_init_args.option(format!(
                        "--add-exports={}",
                        Template::from_str(opt_string)?.expand(&jvm_map)?
                    ));
                    reaches_add_exports = false;
                    continue;
                }
                jvm_init_args =
                    jvm_init_args.option(Template::from_str(opt_string)?.expand(&jvm_map)?);
            }
            JvmArguments::WithRules { rules: _, value } => {
                for opt_string in value {
                    jvm_init_args =
                        jvm_init_args.option(Template::from_str(opt_string)?.expand(&jvm_map)?);
                }
            }
        }
    }

    let jvm = jni::vm::JavaVM::new(jvm_init_args.build()?)?;

    let mut game_map = HashMap::from([
        ("auth_player_name", auth_player_name.to_owned()),
        ("version_name", manifest.id().clone()),
        ("game_directory", game_directory().display().to_string()),
        ("assets_root", assets_root().display().to_string()),
        ("assets_index_name", manifest.assets().clone()),
        ("version_type", manifest.r#type().to_string()),
    ]);
    if let Some(auth_uuid) = auth_uuid {
        game_map.insert("auth_uuid", auth_uuid.to_owned());
    }
    if let Some(auth_access_token) = Some(auth_access_token) {
        game_map.insert("auth_access_token", auth_access_token.to_owned());
    }
    if let Some(clientid) = clientid {
        game_map.insert("clientid", clientid.to_owned());
    }
    if let Some(auth_xuid) = auth_xuid {
        game_map.insert("auth_xuid", auth_xuid.to_owned());
    }
    if let Some(custom_resolution) = custom_resolution {
        game_map.insert(
            "resolution_width",
            custom_resolution.resolution_width.to_string(),
        );
        game_map.insert(
            "resolution_height",
            custom_resolution.resolution_height.to_string(),
        );
    }
    if let Some(quick_play_path) = quick_play_path {
        game_map.insert("quickPlayPath", quick_play_path.to_owned());
    }
    if let Some(quick_play_singleplayer) = quick_play_singleplayer {
        game_map.insert("quickPlaySingleplayer", quick_play_singleplayer.to_owned());
    }
    if let Some(quick_play_multiplayer) = quick_play_multiplayer {
        game_map.insert("quickPlayMultiplayer", quick_play_multiplayer.to_owned());
    }
    if let Some(quick_play_realms) = quick_play_realms {
        game_map.insert("quickPlayRealms", quick_play_realms.to_owned());
    }

    let mut game_arguments = Vec::new();
    for game in manifest.arguments().game() {
        if !game.is_allowed(
            is_demo,
            custom_resolution.is_some(),
            quick_play_path.is_some(),
            quick_play_singleplayer.is_some(),
            quick_play_multiplayer.is_some(),
            quick_play_realms.is_some(),
        ) {
            continue;
        }

        match game {
            crate::manifest::version::GameArguments::Value(opt_string) => match &**opt_string {
                "--uuid" if auth_uuid.is_none() => continue,
                // "--accessToken" if auth_access_token.is_none() => continue,
                "--clientId" if clientid.is_none() => continue,
                "--xuid" if auth_xuid.is_none() => continue,
                _ => game_arguments.push(match Template::from_str(opt_string)?.expand(&game_map) {
                    Ok(x) => x,
                    Err(ExpandError::NoSuchVariable(_)) => continue,
                    #[allow(unreachable_patterns)]
                    Err(e) => return Err(e.into()),
                }),
            },
            crate::manifest::version::GameArguments::WithRules { rules: _, value } => {
                for opt_string in value {
                    game_arguments.push(Template::from_str(opt_string)?.expand(&game_map)?);
                }
            }
        }
    }

    Ok((
        std::thread::spawn({
            let jvm = jvm.clone();
            let main_class = manifest.main_class().clone();
            move || {
                let jvm = jvm;
                let main_class = main_class.replace('.', "/");
                jvm.attach_current_thread::<_, (), jni::errors::Error>(|env| {
                    let arg_array =
                        JObjectArray::<JString>::new(env, game_arguments.len(), JString::null())?;
                    for (i, arg) in game_arguments.into_iter().enumerate() {
                        let arg = JString::new(env, arg)?;
                        arg_array.set_element(env, i, arg)?;
                    }
                    let class = env.find_class(JNIString::new(&main_class))?;
                    env.call_static_method(
                        class,
                        jni_str!("main"),
                        jni_sig!("([Ljava/lang/String;)V"),
                        &[JValue::Object(arg_array.as_ref())],
                    )?;
                    Ok(())
                })
            }
        }),
        jvm,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch() {
        let (handle, _vm) = super::launch(
            "26.3", "player", None, "aaa", None, None, false, None, false, false, None, None, None,
            None,
        )
        .unwrap();
        handle.join().unwrap().unwrap();
    }
}
