//! 受控内置资产标识；命名空间供编译器和构建计划使用，不属于用户资产符号。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum BuiltinImage {
    BrowserFolder,
    BrowserFile,
}
impl BuiltinImage {
    pub const ALL: [Self; 2] = [Self::BrowserFolder, Self::BrowserFile];
    pub fn id(self) -> &'static str {
        match self {
            Self::BrowserFolder => "browser-folder",
            Self::BrowserFile => "browser-file",
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Self::BrowserFolder => "__hmi_browser_folder",
            Self::BrowserFile => "__hmi_browser_file",
        }
    }
    pub fn source_sha256(self) -> &'static str {
        match self {
            Self::BrowserFolder => {
                "b7db46948ba8cfbae0ed959e8dc5a7b2e8dde5f33122b40461b15ae1d12ffa33"
            }
            Self::BrowserFile => "68fadc3266bcdfda3bcdce42985535773c30505926402627b5803b032696453f",
        }
    }
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|b| b.id() == id)
    }
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|b| b.key() == key)
    }
}
