//! 各工具共用的错误、诊断和内容摘要，不依赖具体构建阶段。

pub mod cli;
pub mod controls;
pub mod diagnostic;
pub mod file;
pub mod model;
pub mod target;

use sha2::{Digest, Sha256};

/// 工具链错误；结构化诊断可以保留源码位置和稳定的错误代码。
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Message(String),
    #[error("{0}")]
    Structured(Box<diagnostic::Diagnostic>),
}

pub type Result<T> = std::result::Result<T, Error>;

/// 创建具有稳定代码的错误，供 CLI 输出和自动化调用共同使用。
pub fn error(code: &str, message: impl Into<String>) -> Error {
    diagnostic::Diagnostic::error(code, message).into()
}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::Message(value.to_string())
    }
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
            _ => None,
        }
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
