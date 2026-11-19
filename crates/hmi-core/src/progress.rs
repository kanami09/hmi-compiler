//! 编码后端的可选进度回调；计数不参与产物内容，也不直接输出日志。
use serde::Serialize;
use std::sync::Arc;

#[derive(Clone, Debug, Serialize)]
pub struct ProgressValue {
    pub phase: &'static str,
    pub current: u64,
    pub total: Option<u64>,
    pub unit: &'static str,
}
#[derive(Clone, Default)]
pub struct Progress(Option<Arc<dyn Fn(ProgressValue) + Send + Sync>>);
impl Progress {
    pub fn new(callback: impl Fn(ProgressValue) + Send + Sync + 'static) -> Self {
        Self(Some(Arc::new(callback)))
    }
    pub fn set(&self, phase: &'static str, current: u64, total: Option<u64>, unit: &'static str) {
        if let Some(callback) = &self.0 {
            callback(ProgressValue {
                phase,
                current,
                total: total.filter(|&n| n >= current),
                unit,
            });
        }
    }
}
