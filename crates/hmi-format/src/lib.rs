//! HMI 二进制结构和校验规则，供解析器、编译器和链接器共用。
//!
//! 此库不读取工程目录、不管理缓存，也不调用媒体后端。

pub mod binary;
pub mod container;
pub mod page;
pub mod project;

pub use hmi_core::{Error, Result, diagnostic};
