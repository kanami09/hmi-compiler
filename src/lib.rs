//! v0.1 工程导出和重建入口，迁移期间复用新的共享格式库。

pub mod build;
mod cache;
pub mod diff;
mod script;
pub mod warning;
pub mod workspace;

// 保留旧入口的导出路径，让迁移中的测试继续验证同一套二进制实现。
pub use hmi_core::{Error, Result, diagnostic, hex, sha256};
pub use hmi_format::{binary, container, page, project};
