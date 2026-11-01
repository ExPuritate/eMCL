use std::{ffi::OsString, os::windows::ffi::OsStringExt, path::PathBuf, sync::OnceLock};

#[cfg(windows)]
fn get_current_lib_path() -> PathBuf {
    use windows::{
        Win32::{
            Foundation::{HMODULE, MAX_PATH},
            System::LibraryLoader::{
                GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS,
                GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT, GetModuleFileNameW,
                GetModuleHandleExW,
            },
        },
        core::PCWSTR,
    };

    let mut buf = vec![0u16; MAX_PATH as usize];

    #[allow(nonstandard_style)]
    unsafe {
        let mut hModule = HMODULE::default();
        GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            PCWSTR(get_current_lib_path as *const i8 as *const _),
            &mut hModule,
        )
        .unwrap();
        let ret = GetModuleFileNameW(Some(hModule), &mut buf);
        assert_ne!(ret, 0);
        buf.set_len(ret as usize);
        PathBuf::from(OsString::from_wide(&buf))
    }
}

#[cfg(unix)]
fn get_current_lib_path() -> PathBuf {
    let symbol_address = get_current_lib_path as *const libc::c_void;

    unsafe {
        use std::ffi::CStr;

        let mut info: libc::Dl_info = std::mem::zeroed();
        // cSpell:disable-next-line
        assert!(libc::dladdr(symbol_address, &mut info) != 0 && !info.dli_fname.is_null());
        PathBuf::from(CStr::from_ptr(info.dli_fname).to_str().unwrap())
    }
}

pub fn home_dir() -> &'static PathBuf {
    static CACHE: OnceLock<PathBuf> = OnceLock::new();
    CACHE.get_or_init(|| {
        if cfg!(test) {
            std::env::current_dir().unwrap()
        } else {
            let mut res = get_current_lib_path();
            res.pop();
            res
        }
    })
}

pub fn configuration_path() -> PathBuf {
    home_dir().join("Configuration.json")
}

pub fn game_directory() -> &'static PathBuf {
    static CACHE: OnceLock<PathBuf> = OnceLock::new();
    CACHE.get_or_init(|| {
        let mut result = home_dir().clone();
        result.push(".minecraft");
        std::fs::create_dir_all(&result).unwrap();
        result
    })
}

pub fn assets_root() -> &'static PathBuf {
    static CACHE: OnceLock<PathBuf> = OnceLock::new();
    CACHE.get_or_init(|| {
        let mut result = game_directory().clone();
        result.push("assets");
        std::fs::create_dir_all(&result).unwrap();
        result
    })
}

pub fn dir_to_version(id: &str) -> PathBuf {
    let mut result = game_directory().clone();
    result.push("versions");
    result.push(id);
    std::fs::create_dir_all(&result).unwrap();
    result
}

pub fn dir_to_asset_index() -> PathBuf {
    let mut result = assets_root().clone();
    result.push("indexes");
    std::fs::create_dir_all(&result).unwrap();
    result
}

pub fn path_to_asset_object(hash: &str) -> PathBuf {
    let mut result = assets_root().clone();
    result.push("objects");
    result.push(&hash[0..2]);
    std::fs::create_dir_all(&result).unwrap();
    result.push(hash);
    result
}

pub fn path_to_library(child: &str) -> PathBuf {
    let mut result = game_directory().clone();
    result.push("libraries");
    result.push(child);
    std::fs::create_dir_all(result.parent().unwrap()).unwrap();
    result
}
