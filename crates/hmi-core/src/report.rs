//! 与显示方式无关的执行报告。任务标识只在本次执行中有效，不参与工程对象身份。
use crate::{Result, diagnostic::Diagnostic, file, model::PageSource, sha256};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Instant,
};

type Listener = Arc<dyn Fn(Value) + Send + Sync>;

#[derive(Clone)]
pub struct Report {
    inner: Arc<Mutex<State>>,
    listener: Option<Listener>,
}
struct State {
    started: Instant,
    project: Option<Value>,
    planned: BTreeMap<String, usize>,
    tasks: Vec<TaskRecord>,
    outputs: Vec<Output>,
    removed: Vec<PathBuf>,
    retained: Vec<PathBuf>,
    sources: Vec<Source>,
    stage: String,
    query: Option<String>,
    parallelism: Option<Value>,
}
#[derive(Clone, Serialize)]
struct TaskRecord {
    id: usize,
    stage: String,
    group: Option<String>,
    subject: String,
    action: String,
    state: String,
    elapsed_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    progress: Option<crate::progress::ProgressValue>,
    #[serde(skip)]
    started: Instant,
}
#[derive(Clone, Serialize)]
struct Output {
    kind: String,
    path: PathBuf,
    state: String,
    size_bytes: Option<u64>,
    sha256: Option<String>,
}
struct Source {
    path: PathBuf,
    page: Option<String>,
    object: Option<String>,
    event: Option<String>,
}

impl Default for Report {
    fn default() -> Self {
        Self::new(None)
    }
}
impl Report {
    pub fn new(listener: Option<Listener>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(State {
                started: Instant::now(),
                project: None,
                planned: BTreeMap::new(),
                tasks: vec![],
                outputs: vec![],
                removed: vec![],
                retained: vec![],
                sources: vec![],
                stage: "load".into(),
                query: None,
                parallelism: None,
            })),
            listener,
        }
    }
    fn emit(&self, event: Value) {
        if let Some(listener) = &self.listener {
            listener(event);
        }
    }
    pub fn elapsed_ms(&self) -> u64 {
        milliseconds(self.inner.lock().unwrap().started.elapsed())
    }
    pub fn project(&self, name: &str, root: &Path) -> Result<()> {
        self.inner.lock().unwrap().project =
            Some(json!({"name":name,"root":file::absolute(root)?}));
        Ok(())
    }
    pub fn project_value(&self) -> Option<Value> {
        self.inner.lock().unwrap().project.clone()
    }
    pub fn plan(&self, group: &str, total: usize) {
        self.inner
            .lock()
            .unwrap()
            .planned
            .insert(group.into(), total);
    }
    pub fn parallelism(&self, jobs: usize, media_jobs: usize) {
        self.inner.lock().unwrap().parallelism = Some(json!({"jobs":jobs,"media_jobs":media_jobs}));
    }
    pub fn add_planned(&self, group: &str, count: usize) {
        *self
            .inner
            .lock()
            .unwrap()
            .planned
            .entry(group.into())
            .or_default() += count;
    }
    pub fn query(&self, text: String) {
        self.inner.lock().unwrap().query = Some(text);
    }
    pub fn query_text(&self) -> Option<String> {
        self.inner.lock().unwrap().query.clone()
    }
    pub fn note(&self, message: impl Into<String>) {
        self.emit(json!({"event":"detail","message":message.into()}));
    }
    pub fn stage(&self, stage: &str, action: &str, subject: impl Into<String>) {
        self.inner.lock().unwrap().stage = stage.into();
        self.emit(
            json!({"event":"stage_started","stage":stage,"action":action,"subject":subject.into()}),
        );
    }
    pub fn set_stage(&self, stage: &str) {
        self.inner.lock().unwrap().stage = stage.into();
    }
    pub fn task(
        &self,
        stage: &str,
        group: Option<&str>,
        action: &str,
        subject: impl Into<String>,
    ) -> Task {
        let mut state = self.inner.lock().unwrap();
        state.stage = stage.into();
        let id = state.tasks.len();
        let record = TaskRecord {
            id,
            stage: stage.into(),
            group: group.map(Into::into),
            action: action.into(),
            subject: subject.into(),
            state: "running".into(),
            elapsed_ms: 0,
            progress: None,
            started: Instant::now(),
        };
        let event = json!({"event":"task_started","task":record});
        state.tasks.push(record);
        drop(state);
        self.emit(event);
        Task {
            report: self.clone(),
            id,
            done: false,
        }
    }
    pub fn progress(&self) {
        let tasks = self
            .inner
            .lock()
            .unwrap()
            .tasks
            .iter()
            .filter(|t| t.state == "running")
            .cloned()
            .collect::<Vec<_>>();
        for task in tasks {
            self.emit(
                json!({"event":"task_progress","task_id":task.id,"stage":task.stage,
                "subject":task.subject,"action":task.action,"elapsed_ms":milliseconds(task.started.elapsed()),"progress":task.progress}),
            );
        }
    }
    pub fn plan_output(&self, kind: &str, path: &Path) -> Result<()> {
        let path = file::absolute(path)?;
        let mut state = self.inner.lock().unwrap();
        if !state.outputs.iter().any(|o| o.path == path) {
            state.outputs.push(Output {
                kind: kind.into(),
                path,
                state: "not_written".into(),
                size_bytes: None,
                sha256: None,
            });
        }
        Ok(())
    }
    /// 只在实际发布成功后调用；不通过“文件存在”推断本次写入成功。
    pub fn written(&self, kind: &str, path: &Path, bytes: Option<&[u8]>) -> Result<()> {
        self.plan_output(kind, path)?;
        let path = file::absolute(path)?;
        let output = {
            let mut state = self.inner.lock().unwrap();
            let output = state.outputs.iter_mut().find(|o| o.path == path).unwrap();
            output.state = "written".into();
            if let Some(bytes) = bytes {
                output.size_bytes = Some(bytes.len() as u64);
                output.sha256 = Some(sha256(bytes));
            }
            output.clone()
        };
        self.emit(json!({"event":"output_written","output":output}));
        Ok(())
    }
    pub fn removed(&self, path: &Path) {
        self.inner.lock().unwrap().removed.push(path.into());
    }
    pub fn retained(&self, path: &Path) {
        self.inner.lock().unwrap().retained.push(path.into());
    }
    pub fn retained_count(&self) -> usize {
        self.inner.lock().unwrap().retained.len()
    }
    pub fn source(
        &self,
        path: &Path,
        page: Option<&str>,
        object: Option<&str>,
        event: Option<&str>,
    ) {
        if let Ok(path) = file::absolute(path) {
            self.inner.lock().unwrap().sources.push(Source {
                path,
                page: page.map(Into::into),
                object: object.map(Into::into),
                event: event.map(Into::into),
            });
        }
    }
    pub fn page_sources(&self, path: &Path) {
        self.source(path, None, None, None);
        // 映射仅辅助定位；源码错误由编译器报告，此处失败不能改变诊断顺序。
        if let Ok(page) = file::read_toml::<PageSource>(path) {
            for (event, source) in &page.events {
                self.source(
                    &path.parent().unwrap().join(source),
                    Some(&page.name),
                    Some(&page.name),
                    Some(event),
                );
            }
            for object in &page.objects {
                for (event, source) in &object.events {
                    self.source(
                        &path.parent().unwrap().join(source),
                        Some(&page.name),
                        Some(&object.name),
                        Some(event),
                    );
                }
            }
        }
    }
    pub fn normalize(&self, diagnostic: &mut Diagnostic) {
        let state = self.inner.lock().unwrap();
        if diagnostic.stage.is_none() {
            diagnostic.stage = Some(state.stage.clone());
        }
        let Some(label) = diagnostic.location.file.clone() else {
            return;
        };
        let path = Path::new(&label);
        if path.is_absolute() {
            if let Ok(path) = file::absolute(path) {
                diagnostic.location.file = Some(path.display().to_string());
            }
            return;
        }
        let candidates = state
            .sources
            .iter()
            .filter(|s| {
                s.path.file_name() == path.file_name()
                    && diagnostic
                        .location
                        .page
                        .as_ref()
                        .is_none_or(|p| s.page.as_ref().is_none_or(|sp| sp == p))
                    && diagnostic
                        .location
                        .object
                        .as_ref()
                        .is_none_or(|o| s.object.as_ref().is_none_or(|so| so == o))
                    && diagnostic
                        .location
                        .event
                        .as_ref()
                        .is_none_or(|e| s.event.as_ref().is_none_or(|se| se == e))
            })
            .map(|s| s.path.clone())
            .collect::<std::collections::BTreeSet<_>>();
        if candidates.len() == 1 {
            diagnostic.location.file = Some(candidates.first().unwrap().display().to_string());
        } else {
            // 独立链接可能没有源码，不能把目标中的逻辑文件名伪造成当前目录的文件。
            diagnostic
                .details
                .insert("source_name".into(), label.into());
            diagnostic.location.file = None;
        }
    }
    pub fn result(&self, mut value: Value) -> Value {
        let state = self.inner.lock().unwrap();
        if value.is_null()
            && state.project.is_none()
            && state.outputs.is_empty()
            && state.tasks.is_empty()
        {
            return value;
        }
        if !value.is_object() {
            value = if value.is_null() {
                json!({})
            } else {
                json!({"value":value})
            };
        }
        let mut summary = serde_json::Map::new();
        for (group, total) in &state.planned {
            let count = |status: &str| {
                state
                    .tasks
                    .iter()
                    .filter(|t| t.group.as_deref() == Some(group) && t.state == status)
                    .count()
            };
            let processed = count("processed");
            let reused = count("reused");
            let failed = count("failed");
            summary.insert(
                group.clone(),
                json!({"total":total,"processed":processed,"reused":reused,"failed":failed,
                "not_run":total.saturating_sub(processed+reused+failed)}),
            );
        }
        value["elapsed_ms"] = milliseconds(state.started.elapsed()).into();
        value["summary"] = summary.into();
        value["outputs"] = json!(state.outputs);
        if let Some(parallelism) = &state.parallelism {
            value["parallelism"] = parallelism.clone();
        }
        if !state.removed.is_empty() {
            value["removed_files"] = json!(state.removed);
        }
        if !state.retained.is_empty() {
            value["retained_files"] = json!(state.retained);
        }
        value
    }
}
pub struct Task {
    report: Report,
    id: usize,
    done: bool,
}
impl Task {
    pub fn progress_handle(&self) -> crate::progress::Progress {
        let report = self.report.clone();
        let id = self.id;
        crate::progress::Progress::new(move |progress| {
            let mut state = report.inner.lock().unwrap();
            let task = &mut state.tasks[id];
            if task.state == "running" {
                task.progress = Some(progress);
            }
        })
    }
    pub fn subject(&self, subject: impl Into<String>) {
        self.report.inner.lock().unwrap().tasks[self.id].subject = subject.into();
    }
    /// 错误绑定产生它的任务，后续并行执行也不依赖全局“最近阶段”。
    pub fn error(&self, error: crate::Error) -> crate::Error {
        let stage = self.report.inner.lock().unwrap().tasks[self.id]
            .stage
            .clone();
        error.with_stage(&stage)
    }
    /// 缓存复用与成功处理互斥计数；原始编码复用仍属于一次实际媒体处理。
    pub fn finish(mut self, reused: bool) {
        self.end(if reused { "reused" } else { "processed" });
    }
    pub fn failed(mut self) {
        self.end("failed");
    }
    pub fn action(&self, action: &str) {
        let record = {
            let mut state = self.report.inner.lock().unwrap();
            let task = &mut state.tasks[self.id];
            if task.action == action {
                return;
            }
            task.action = action.into();
            task.clone()
        };
        self.report
            .emit(json!({"event":"task_action","task":record}));
    }
    fn end(&mut self, status: &str) {
        let record = {
            let mut state = self.report.inner.lock().unwrap();
            let task = &mut state.tasks[self.id];
            task.state = status.into();
            task.elapsed_ms = milliseconds(task.started.elapsed());
            task.clone()
        };
        self.done = true;
        self.report
            .emit(json!({"event":"task_finished","task":record}));
    }
}
impl Drop for Task {
    fn drop(&mut self) {
        if !self.done {
            self.end("failed");
        }
    }
}
fn milliseconds(duration: std::time::Duration) -> u64 {
    duration.as_millis().try_into().unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn terminal_counts_partition_planned_work_even_after_failure() {
        let report = Report::default();
        report.plan("pages", 4);
        report
            .task("compile", Some("pages"), "编译", "页面 a")
            .finish(false);
        report
            .task("compile", Some("pages"), "cache_hit", "页面 b")
            .finish(true);
        drop(report.task("compile", Some("pages"), "编译", "页面 c"));
        let result = report.result(Value::Null);
        assert_eq!(
            result["summary"]["pages"],
            json!({"total":4,"processed":1,"reused":1,"failed":1,"not_run":1})
        );
    }
    #[test]
    fn existing_output_is_not_reported_as_a_successful_publication() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("out.HMI");
        std::fs::write(&path, b"old").unwrap();
        let report = Report::default();
        report.plan_output("hmi", &path).unwrap();
        let result = report.result(Value::Null);
        assert_eq!(result["outputs"][0]["state"], "not_written");
        assert!(result["outputs"][0]["sha256"].is_null());
        std::fs::write(&path, b"new").unwrap();
        report.written("hmi", &path, Some(b"new")).unwrap();
        let result = report.result(Value::Null);
        assert_eq!(result["outputs"][0]["state"], "written");
        assert_eq!(result["outputs"][0]["sha256"], sha256(b"new"));
    }
    #[test]
    fn ambiguous_artifact_source_is_preserved_without_inventing_an_absolute_path() {
        let tmp = tempfile::tempdir().unwrap();
        let report = Report::default();
        report.source(
            &tmp.path().join("a/up.s"),
            Some("a"),
            Some("b0"),
            Some("up"),
        );
        report.source(
            &tmp.path().join("b/up.s"),
            Some("b"),
            Some("b0"),
            Some("up"),
        );
        let mut d = Diagnostic::error("test", "test");
        d.location.file = Some("up.s".into());
        report.normalize(&mut d);
        assert!(d.location.file.is_none());
        assert_eq!(d.details["source_name"], "up.s");
        d.location.file = Some("up.s".into());
        d.location.page = Some("b".into());
        report.normalize(&mut d);
        assert_eq!(
            d.location.file.unwrap(),
            file::absolute(&tmp.path().join("b/up.s"))
                .unwrap()
                .display()
                .to_string()
        );
    }
}
