//! 陶晶驰 ver21234 工程的保真解析、工作目录导出和回编。

pub mod binary;
pub mod build;
mod cache;
pub mod container;
pub mod diagnostic;
pub mod diff;
pub mod page;
pub mod project;
mod script;
pub mod warning;
pub mod workspace;

use sha2::{Digest, Sha256};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Message(String),
    #[error("{0}")]
    Structured(Box<diagnostic::Diagnostic>),
}

pub type Result<T> = std::result::Result<T, Error>;

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
    pub fn as_diagnostic(&self) -> Option<&diagnostic::Diagnostic> {
        match self {
            Self::Structured(diagnostic) => Some(diagnostic),
            _ => None,
        }
    }
}

pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
