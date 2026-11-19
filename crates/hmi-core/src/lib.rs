//! 各工具共用的错误、诊断和内容摘要，不依赖具体构建阶段。

pub mod asset;
pub mod builtin;
pub mod cli;
pub mod controls;
pub mod dependency;
pub mod deployment;
pub mod diagnostic;
pub mod file;
pub mod icon_map;
pub mod model;
pub mod progress;
pub mod report;
pub mod target;

use sha2::{Digest, Sha256};

/// 工具链错误；结构化诊断可以保留源码位置和稳定的错误代码。
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Message(String),
    #[error("{0}")]
    Structured(Box<diagnostic::Diagnostic>),
    #[error("{}", .0.first().map(|d| d.message.as_str()).unwrap_or("执行失败"))]
    Diagnostics(Vec<diagnostic::Diagnostic>),
}

pub type Result<T> = std::result::Result<T, Error>;

/// 创建具有稳定代码的错误，供 CLI 输出和自动化调用共同使用。
pub fn error(code: &str, message: impl Into<String>) -> Error {
    diagnostic::Diagnostic::error(code, message).into()
}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        io_error("io", value.to_string(), &value)
    }
}

/// 操作系统错误保留可判定的类别和原始错误号，调用方无须解析本地化说明。
pub fn io_error(code: &str, message: String, cause: &std::io::Error) -> Error {
    let mut diagnostic = diagnostic::Diagnostic::error(code, message);
    let kind = match cause.kind() {
        std::io::ErrorKind::NotFound => "not_found",
        std::io::ErrorKind::PermissionDenied => "permission_denied",
        std::io::ErrorKind::AlreadyExists => "already_exists",
        std::io::ErrorKind::InvalidInput => "invalid_input",
        _ => "other",
    };
    diagnostic.details.insert("io_kind".into(), kind.into());
    if let Some(code) = cause.raw_os_error() {
        diagnostic
            .details
            .insert("os_error_code".into(), code.into());
    }
    diagnostic.into()
}

impl From<diagnostic::Diagnostic> for Error {
    fn from(value: diagnostic::Diagnostic) -> Self {
        Self::Structured(Box::new(value))
    }
}

impl Error {
    /// 读取已有诊断，避免阶段边界把错误代码和源码位置降为普通字符串。
    pub fn as_diagnostic(&self) -> Option<&diagnostic::Diagnostic> {
        match self {
            Self::Structured(diagnostic) => Some(diagnostic),
            Self::Diagnostics(diagnostics) => diagnostics
                .iter()
                .find(|d| d.severity == diagnostic::Severity::Error)
                .or_else(|| diagnostics.first()),
            _ => None,
        }
    }

    /// 阶段边界保留全部诊断，避免失败时只剩首条错误、丢失此前警告。
    pub fn diagnostics(&self) -> Vec<diagnostic::Diagnostic> {
        match self {
            Self::Structured(diagnostic) => vec![diagnostic.as_ref().clone()],
            Self::Diagnostics(diagnostics) => diagnostics.clone(),
            Self::Message(message) => vec![diagnostic::Diagnostic::error("execution", message)],
        }
    }

    pub fn with_stage(self, stage: &str) -> Self {
        let mut diagnostics = self.diagnostics();
        for diagnostic in &mut diagnostics {
            diagnostic.stage.get_or_insert_with(|| stage.into());
        }
        Self::Diagnostics(diagnostics)
    }
}

/// 对内容计算摘要；缓存和中间文件不依赖修改时间判断输入是否改变。
pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// 以小写十六进制显示原始字段。
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
