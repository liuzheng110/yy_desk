use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataDirMode {
    Default,
    EnvOverride,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataDirResolution {
    pub data_dir: PathBuf,
    pub default_data_dir: PathBuf,
    pub mode: DataDirMode,
}

pub fn resolve_data_dir_with_mode(default_app_data_dir: PathBuf) -> DataDirResolution {
    let env_data_dir = std::env::var_os("MYAPP_DATA_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);

    resolve_data_dir_from_inputs(default_app_data_dir, env_data_dir)
}

fn resolve_data_dir_from_inputs(default_app_data_dir: PathBuf, env_data_dir: Option<PathBuf>) -> DataDirResolution {
    if let Some(env_dir) = env_data_dir {
        return DataDirResolution {
            data_dir: env_dir,
            default_data_dir: default_app_data_dir,
            mode: DataDirMode::EnvOverride,
        };
    }

    DataDirResolution {
        data_dir: default_app_data_dir.clone(),
        default_data_dir: default_app_data_dir,
        mode: DataDirMode::Default,
    }
}

#[cfg(test)]
mod tests {
    use super::{resolve_data_dir_from_inputs, DataDirMode};
    use std::path::PathBuf;

    #[test]
    fn env_override_uses_custom_data_dir() {
        let default_dir = PathBuf::from(r"C:\Users\Administrator\AppData\Roaming\com.myapp.app");
        let env_dir = PathBuf::from(r"E:\MyAppData");

        let resolution = resolve_data_dir_from_inputs(default_dir, Some(env_dir.clone()));

        assert_eq!(resolution.data_dir, env_dir);
        assert_eq!(resolution.mode, DataDirMode::EnvOverride);
    }

    #[test]
    fn default_mode_uses_app_data_dir() {
        let default_dir = PathBuf::from(r"C:\Users\Administrator\AppData\Roaming\com.myapp.app");

        let resolution = resolve_data_dir_from_inputs(default_dir.clone(), None);

        assert_eq!(resolution.data_dir, default_dir);
        assert_eq!(resolution.mode, DataDirMode::Default);
    }
}
