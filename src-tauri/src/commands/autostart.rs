//! 开机自启动命令（autostart）。
//!
//! - Windows：HKCU\Software\Microsoft\Windows\CurrentVersion\Run 写入当前 exe 路径
//! - Linux：  XDG Autostart（~/.config/autostart/myapp.desktop）
//! - macOS：  ~/Library/LaunchAgents/com.myapp.app.plist
//!
//! 不依赖 tauri-plugin-autostart，避免新增第三方依赖与权限配置。

// Preserve the existing registry value for this application; derived apps use their own name.
const AUTOSTART_VALUE_NAME: &str = app_platform::identity::APP_NAME;

/// 查询开机自启动是否开启。
#[tauri::command]
pub fn get_autostart_enabled() -> bool {
    autostart_enabled().unwrap_or(false)
}

/// 设置/取消开机自启动。
#[tauri::command]
pub fn set_autostart_enabled(enabled: bool) -> Result<(), String> {
    set_autostart(enabled).map_err(|e| format!("set_autostart failed: {e}"))
}

fn autostart_enabled() -> std::io::Result<bool> {
    imp::enabled()
}

fn set_autostart(enabled: bool) -> std::io::Result<()> {
    imp::set(enabled)
}

#[cfg(target_os = "windows")]
mod imp {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::ptr;
    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    use windows_sys::Win32::System::Registry::*;

    const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const VALUE_NAME: &str = super::AUTOSTART_VALUE_NAME;

    fn to_utf16(s: &str) -> Vec<u16> {
        OsStr::new(s).encode_wide().chain(Some(0)).collect()
    }

    fn open_run_key(access: u32) -> std::io::Result<HKEY> {
        let key = to_utf16(RUN_KEY);
        let mut hkey = ptr::null_mut();
        let rc = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, key.as_ptr(), 0, access, &mut hkey) };
        if rc != ERROR_SUCCESS {
            return Err(std::io::Error::from_raw_os_error(rc as i32));
        }
        Ok(hkey)
    }

    pub fn enabled() -> std::io::Result<bool> {
        let hkey = match open_run_key(KEY_QUERY_VALUE) {
            Ok(h) => h,
            Err(_) => return Ok(false),
        };
        let name = to_utf16(VALUE_NAME);
        let mut buf: [u16; 2048] = [0; 2048];
        let mut len = (buf.len() * 2) as u32;
        let mut ty: u32 = 0;
        let rc = unsafe {
            RegQueryValueExW(
                hkey,
                name.as_ptr(),
                ptr::null(),
                &mut ty,
                buf.as_mut_ptr() as *mut u8,
                &mut len,
            )
        };
        unsafe { RegCloseKey(hkey) };
        Ok(rc == ERROR_SUCCESS && ty == REG_SZ && len > 0)
    }

    pub fn set(enabled: bool) -> std::io::Result<()> {
        let hkey = open_run_key(KEY_SET_VALUE)?;
        let name = to_utf16(VALUE_NAME);
        let rc = if enabled {
            let exe = std::env::current_exe()?;
            let value = format!("\"{}\"", exe.to_string_lossy());
            let data = to_utf16(&value);
            unsafe {
                RegSetValueExW(
                    hkey,
                    name.as_ptr(),
                    0,
                    REG_SZ,
                    data.as_ptr() as *const u8,
                    (data.len() * 2) as u32,
                )
            }
        } else {
            unsafe { RegDeleteValueW(hkey, name.as_ptr()) }
        };
        unsafe { RegCloseKey(hkey) };
        match rc {
            ERROR_SUCCESS | ERROR_FILE_NOT_FOUND => Ok(()),
            other => Err(std::io::Error::from_raw_os_error(other as i32)),
        }
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use std::path::PathBuf;

    fn desktop_file() -> PathBuf {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                let home = std::env::var_os("HOME").unwrap_or_default();
                PathBuf::from(home).join(".config")
            });
        base.join("autostart")
            .join(format!("{}.desktop", app_platform::identity::APP_ID))
    }

    pub fn enabled() -> std::io::Result<bool> {
        Ok(desktop_file().exists())
    }

    pub fn set(enabled: bool) -> std::io::Result<()> {
        let path = desktop_file();
        if enabled {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let exe = std::env::current_exe()?;
            let content = format!(
                "[Desktop Entry]\nType=Application\nName={}\nExec={}\nX-GNOME-Autostart-enabled=true\n",
                app_platform::identity::APP_NAME,
                exe.to_string_lossy()
            );
            std::fs::write(path, content)
        } else {
            match std::fs::remove_file(&path) {
                Ok(_) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(e),
            }
        }
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use std::path::PathBuf;

    fn plist_path() -> PathBuf {
        let home = std::env::var_os("HOME").unwrap_or_default();
        PathBuf::from(home)
            .join("Library")
            .join("LaunchAgents")
            .join(format!("{}.plist", app_platform::identity::APP_ID))
    }

    fn xml_escape(s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    }

    pub fn enabled() -> std::io::Result<bool> {
        Ok(plist_path().exists())
    }

    pub fn set(enabled: bool) -> std::io::Result<()> {
        let path = plist_path();
        if enabled {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let exe = std::env::current_exe()?;
            let content = format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
</dict>
</plist>
"#,
                xml_escape(app_platform::identity::APP_ID),
                xml_escape(&exe.to_string_lossy())
            );
            std::fs::write(path, content)
        } else {
            match std::fs::remove_file(&path) {
                Ok(_) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(e),
            }
        }
    }
}
