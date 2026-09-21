use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager};

use crate::error::{AppError, Result};

/// 便携模式标记文件。放在 exe 同目录即启用，数据改存 exe 旁边的 `data/`。
pub const PORTABLE_MARKER: &str = "glassnote.portable";

#[derive(Debug, Clone)]
pub struct Paths {
    /// 数据根目录
    pub root: PathBuf,
    pub db: PathBuf,
    pub backup_dir: PathBuf,
    pub portable: bool,
}

impl Paths {
    /// 解析数据目录。
    ///
    /// 便携模式优先：exe 同目录若存在标记文件，就用 `<exe目录>/data`，
    /// 这样 U 盘/绿色版拷走即可带走全部数据。
    /// 否则用系统用户数据目录（%APPDATA%\com.glassnote.desktop）。
    pub fn resolve(app: &AppHandle) -> Result<Self> {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf));

        let portable = exe_dir
            .as_ref()
            .map(|d| d.join(PORTABLE_MARKER).exists())
            .unwrap_or(false);

        let root = if portable {
            exe_dir
                .clone()
                .ok_or_else(|| AppError::other("无法定位程序所在目录，便携模式不可用"))?
                .join("data")
        } else {
            app.path()
                .app_data_dir()
                .map_err(|e| AppError::other(format!("无法定位用户数据目录：{e}")))?
        };

        std::fs::create_dir_all(&root)?;
        let backup_dir = root.join("backups");
        std::fs::create_dir_all(&backup_dir)?;

        Ok(Self {
            db: root.join("glassnote.db"),
            backup_dir,
            root,
            portable,
        })
    }
}
