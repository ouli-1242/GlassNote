use serde::{Serialize, Serializer};

/// 应用统一错误类型。
///
/// Tauri 命令返回的 Err 必须可序列化，所以这里统一降级成字符串给前端展示；
/// 真正需要区分处理的地方（比如"玻璃效果不支持"）在前端用返回值而不是 Err 表达，
/// 避免用错误通道传递正常分支。
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("数据库错误：{0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("文件读写错误：{0}")]
    Io(#[from] std::io::Error),

    #[error("JSON 解析错误：{0}")]
    Json(#[from] serde_json::Error),

    #[error("窗口操作错误：{0}")]
    Tauri(#[from] tauri::Error),

    #[error("{0}")]
    Other(String),
}

impl AppError {
    pub fn other(msg: impl Into<String>) -> Self {
        AppError::Other(msg.into())
    }
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

pub type Result<T> = std::result::Result<T, AppError>;
