//! 构建状态只保存派生产物；工程输入始终重新读取摘要，不从缓存补回缺失输入。

use hmi_artifact::{CompiledObject, asset::AssetObject};
use hmi_core::{
    Result, error, file,
    model::{PageSource, ProjectSource},
    sha256,
    target::Target,
};
use hmi_page_compiler::Compilation;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::OnceLock,
};

pub const DIRECTORY: &str = ".build/hmi-cache";
const STATE: &str = ".build/hmi-owned.json";
const LOCK: &str = ".build/hmi.lock";
const SCHEMA: &str = "hmi.cache/1";

#[derive(Default, Debug, Serialize)]
pub struct BuildStats {
    pub compiled: usize,
    pub reused_objects: usize,
    pub encoded: usize,
    pub reused_assets: usize,
}

/// 操作系统锁随句柄关闭而释放；进程异常退出后不需要手动删除锁文件。
pub struct ProjectLock(fs::File);

impl ProjectLock {
    pub fn acquire(root: &Path, inputs: &[PathBuf]) -> Result<Self> {
        let path = root.join(LOCK);
        safe_directory(root, Path::new(".build"))?;
        let protected: Vec<_> = inputs.iter().map(PathBuf::as_path).collect();
        file::protect_output(&path, &protected)?;
        if fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(error("build_directory", "工程锁不能是符号链接"));
        }
        fs::create_dir_all(path.parent().unwrap())?;
        let handle = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&path)?;
        handle.try_lock().map_err(|e| {
            error(
                "project_locked",
                format!("工程正在构建或清理，请稍后重试：{e}"),
            )
        })?;
        Ok(Self(handle))
    }
}

impl Drop for ProjectLock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

/// 只允许使用工程内的真实目录；拒绝将构建状态写进链接或目录联接指向的位置。
fn safe_directory(root: &Path, relative: &Path) -> Result<()> {
    let mut current = root.to_path_buf();
    for part in relative.components() {
        if !matches!(part, std::path::Component::Normal(_)) {
            return Err(error("build_directory", "构建状态路径必须位于工程目录内"));
        }
        current.push(part);
        if let Ok(metadata) = fs::symlink_metadata(&current) {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(file::at_file(
                    error("build_directory", "构建目录不能是文件、符号链接或目录联接"),
                    &current,
                ));
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if metadata.file_attributes() & 0x400 != 0 {
                    return Err(file::at_file(
                        error("build_directory", "构建目录不能是目录联接或其他重解析点"),
                        &current,
                    ));
                }
            }
        }
    }
    Ok(())
}

#[derive(Deserialize)]
struct MediaDependencies {
    source: String,
    original: Option<String>,
}

/// 只发现声明的文件，不解码媒体；clean 因而不依赖编码器或 FFmpeg。
pub fn declared_inputs(root: &Path) -> Result<Vec<PathBuf>> {
    let manifest = root.join("hmi.toml");
    let project: ProjectSource = file::read_toml(&manifest)?;
    let mut inputs = vec![manifest, root.join(&project.program.source)];
    if let Some(original) = project.original {
        inputs.push(root.join(original.bundle));
    }
    for relative in project.pages.sources {
        let path = root.join(relative);
        let page: PageSource = file::read_toml(&path)?;
        for event in page
            .events
            .values()
            .chain(page.objects.iter().flat_map(|o| o.events.values()))
        {
            inputs.push(path.parent().unwrap().join(event));
        }
        inputs.push(path);
    }
    for asset in project.assets.items {
        let path = root.join(asset.source);
        let media: MediaDependencies = file::read_toml(&path)?;
        inputs.push(path.parent().unwrap().join(media.source));
        if let Some(original) = media.original {
            inputs.push(path.parent().unwrap().join(original));
        }
        inputs.push(path);
    }
    inputs.into_iter().map(|p| file::absolute(&p)).collect()
}

#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
struct Dependency {
    path: PathBuf,
    digest: String,
}

#[derive(Deserialize, Serialize)]
struct Entry {
    schema: String,
    key: String,
    inputs: Vec<Dependency>,
    digest: String,
    // 只在实际使用 DLL 时记录；原生包和 original 复用不受后端路径变化影响。
    backend: Option<PathBuf>,
}

#[derive(Default, Deserialize, Serialize)]
struct Ownership {
    files: BTreeMap<String, String>,
}

pub struct Cache {
    root: PathBuf,
    force: bool,
    tool: String,
    snapshots: BTreeMap<PathBuf, String>,
    owned: Ownership,
    pub stats: BuildStats,
    report: hmi_core::report::Report,
    deferred: bool,
}

impl Cache {
    pub fn new(root: &Path, force: bool, inputs: &[PathBuf]) -> Result<Self> {
        safe_directory(root, Path::new(DIRECTORY))?;
        safe_directory(root, Path::new(crate::BUILD_DIRECTORY))?;
        for relative in [".build/hmi-toolchain/obj", ".build/hmi-toolchain/assets"] {
            safe_directory(root, Path::new(relative))?;
        }
        // 分发版本可以保持不变，实际程序变化仍使派生缓存失效。
        static TOOL: OnceLock<String> = OnceLock::new();
        let tool = match TOOL.get() {
            Some(value) => value.clone(),
            None => {
                let value = sha256(&file::read(&std::env::current_exe()?)?);
                TOOL.get_or_init(|| value.clone()).clone()
            }
        };
        let mut cache = Self {
            root: root.into(),
            force,
            tool,
            snapshots: BTreeMap::new(),
            owned: file::read_json(&root.join(STATE)).unwrap_or_default(),
            stats: BuildStats::default(),
            report: hmi_core::report::Report::default(),
            deferred: false,
        };
        for path in inputs {
            cache.snapshot(path)?;
        }
        cache.protect(&root.join(STATE))?;
        Ok(cache)
    }

    fn snapshot(&mut self, path: &Path) -> Result<()> {
        let path = input_path(path)?;
        if !self.snapshots.contains_key(&path) {
            self.snapshots
                .insert(path.clone(), sha256(&file::read(&path)?));
        }
        Ok(())
    }

    pub fn reporting(&mut self, report: &hmi_core::report::Report) {
        self.report = report.clone();
    }

    /// 工作线程只更新各自的登记增量，工程登记文件由主线程合并后写一次。
    pub fn fork(&self) -> Self {
        Self {
            root: self.root.clone(),
            force: self.force,
            tool: self.tool.clone(),
            snapshots: self.snapshots.clone(),
            owned: Ownership::default(),
            stats: BuildStats::default(),
            report: self.report.clone(),
            deferred: true,
        }
    }

    pub fn absorb(&mut self, worker: Self) -> Result<()> {
        let mut changed = None;
        for (path, digest) in worker.snapshots {
            if self.snapshots.get(&path).is_some_and(|old| old != &digest) {
                changed = Some(path);
            } else {
                self.snapshots.insert(path, digest);
            }
        }
        self.owned.files.extend(worker.owned.files);
        self.stats.compiled += worker.stats.compiled;
        self.stats.reused_objects += worker.stats.reused_objects;
        self.stats.encoded += worker.stats.encoded;
        self.stats.reused_assets += worker.stats.reused_assets;
        if let Some(path) = changed {
            return Err(file::at_file(
                error("input_changed", "输入在并行构建期间发生变化，请重试"),
                &path,
            ));
        }
        Ok(())
    }

    pub fn flush(&mut self) -> Result<()> {
        self.record(&[])
    }

    pub fn link_key(&self, inputs: &serde_json::Value) -> Result<String> {
        self.key("link", &self.root.join("hmi.toml"), inputs)
    }
    pub fn load_link(&self) -> Option<Vec<u8>> {
        if self.force {
            None
        } else {
            file::read(&self.root.join(DIRECTORY).join("link-result.hml")).ok()
        }
    }
    pub fn store_link(&mut self, bytes: &[u8]) -> Result<()> {
        self.verify_inputs()?;
        // 只保留最近一次有效链接，避免每次编辑都累计一份完整HMI和外部媒体。
        let path = self.root.join(DIRECTORY).join("link-result.hml");
        self.protect(&path)?;
        file::write_atomic(
            &path,
            bytes,
            &self
                .snapshots
                .keys()
                .map(PathBuf::as_path)
                .collect::<Vec<_>>(),
        )?;
        self.record(&[path])
    }

    fn key(&self, kind: &str, source: &Path, target: &impl Serialize) -> Result<String> {
        Ok(sha256(&file::json(&serde_json::json!({
            "schema":SCHEMA,"root":self.root,"tool":self.tool,"kind":kind,
            "source":file::absolute(source)?,"target":target,
        }))?))
    }

    fn path(&self, key: &str, extension: &str) -> PathBuf {
        self.root.join(DIRECTORY).join(format!("{key}.{extension}"))
    }

    fn load(&self, key: &str, extension: &str) -> Option<(Vec<u8>, Vec<PathBuf>)> {
        if self.force {
            return None;
        }
        let entry: Entry = file::read_json(&self.path(key, "json")).ok()?;
        if entry.schema != SCHEMA || entry.key != key {
            return None;
        }
        if entry.backend.is_some() && entry.backend != hmi_media::ffmpeg::directory().ok() {
            return None;
        }
        for input in &entry.inputs {
            if self.snapshots.get(&input.path) != Some(&input.digest) {
                return None;
            }
        }
        let bytes = file::read(&self.path(key, extension)).ok()?;
        if sha256(&bytes) != entry.digest {
            return None;
        }
        Some((bytes, entry.inputs.into_iter().map(|d| d.path).collect()))
    }

    fn protect(&self, output: &Path) -> Result<()> {
        file::protect_output(
            output,
            &self
                .snapshots
                .keys()
                .map(PathBuf::as_path)
                .collect::<Vec<_>>(),
        )
    }

    /// 产物与状态分别原子替换；中断造成的不匹配在下次读取时按未命中处理。
    fn store(
        &mut self,
        key: &str,
        extension: &str,
        bytes: &[u8],
        inputs: &[PathBuf],
    ) -> Result<()> {
        let mut dependencies = Vec::new();
        let mut backend = None;
        for input in inputs {
            let path = input_path(input)?;
            let digest = self
                .snapshots
                .get(&path)
                .ok_or_else(|| error("input_changed", "构建期间出现未登记的输入，请重试"))?
                .clone();
            if sha256(&file::read(&path)?) != digest {
                return Err(file::at_file(
                    error("input_changed", "输入在构建期间发生变化，请重试"),
                    &path,
                ));
            }
            if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("dll"))
            {
                backend = Some(hmi_media::ffmpeg::directory()?);
            }
            dependencies.push(Dependency { path, digest });
        }
        let entry = Entry {
            schema: SCHEMA.into(),
            key: key.into(),
            inputs: dependencies,
            digest: sha256(bytes),
            backend,
        };
        let artifact = self.path(key, extension);
        let state = self.path(key, "json");
        self.protect(&artifact)?;
        self.protect(&state)?;
        let protected: Vec<_> = self.snapshots.keys().map(PathBuf::as_path).collect();
        file::write_atomic(&artifact, bytes, &protected)?;
        file::write_atomic(&state, &file::json(&entry)?, &protected)?;
        self.record(&[artifact, state])
    }

    pub fn compile(
        &mut self,
        path: &Path,
        target: &Target,
        original: Option<&hmi_artifact::original::OriginalBundle>,
        original_path: Option<&Path>,
        program: bool,
    ) -> Result<Compilation> {
        let group = if program { "program" } else { "pages" };
        let name = if program {
            format!(
                "全局代码 {}",
                path.file_name().unwrap_or_default().to_string_lossy()
            )
        } else {
            let name = file::read_toml::<PageSource>(path)
                .ok()
                .map(|p| p.name)
                .unwrap_or_else(|| path.display().to_string());
            format!("页面 {name}")
        };
        let key = self.key(
            if program { "program" } else { "page" },
            path,
            &(target, original_path),
        )?;
        if let Some((bytes, inputs)) = self.load(&key, "hmo")
            && let Ok(object) = CompiledObject::decode(&bytes)
            && inputs.contains(&input_path(path)?)
            && original_path
                .map(input_path)
                .transpose()?
                .is_none_or(|p| inputs.contains(&p))
        {
            self.stats.reused_objects += 1;
            self.report
                .task(
                    "compile",
                    Some(group),
                    "cache_hit",
                    format!("{name}（构建缓存）"),
                )
                .finish(true);
            return Ok(Compilation { object, inputs });
        }
        let task = self.report.task("compile", Some(group), "compile", name);
        self.report.note(format!("输入：{}", path.display()));
        let mut result = if program {
            hmi_page_compiler::compile_program(path, target)?
        } else {
            hmi_page_compiler::compile_page_with_original(path, target, original)?
        };
        if let Some(original_path) = original_path {
            result.inputs.push(original_path.into());
        }
        self.store(&key, "hmo", &result.object.encode()?, &result.inputs)?;
        self.stats.compiled += 1;
        task.finish(false);
        Ok(result)
    }

    pub fn encode(
        &mut self,
        path: &Path,
        target: &Target,
        name: &str,
    ) -> Result<hmi_media::EncodingResult> {
        // 控件模板、画布和资源符号顺序不影响编码；音频也不依赖屏幕方向。
        #[derive(Deserialize)]
        struct Kind {
            kind: String,
            input: Option<String>,
            original: Option<String>,
        }
        let task = self.report.task(
            "media",
            Some("media"),
            "cache_check",
            format!("媒体 {name}"),
        );
        let kind: Kind = file::read_toml(path)?;
        let subject = format!(
            "{} {name}",
            match kind.kind.as_str() {
                "image" => "图片",
                "font" => "字库",
                "animation" => "动画",
                "video" => "视频",
                "audio" => "音频",
                _ => "媒体",
            }
        );
        task.subject(subject);
        let media_target = serde_json::json!({
            "format": target.format, "encoding": target.encoding,
            "orientation": if kind.kind == "audio" { 0 } else { target.orientation },
            "image_codecs":target.capabilities.image_codecs,
            "font_versions":target.capabilities.font_versions,
            "media_assets":target.capabilities.media_assets,
            "controls": target.capabilities.control_types.iter().filter(|c| matches!(c.as_str(), "audio"|"video"|"animation"|"external_picture")).collect::<Vec<_>>(),
        });
        let key = self.key("asset", path, &media_target)?;
        // 后端输入在转码前采样，只把编码器实际读取的文件加入该资产的缓存依赖。
        for candidate in hmi_media::ffmpeg::dependency_paths()? {
            if kind.input.as_deref().unwrap_or("source") == "source"
                && candidate.is_file()
                && matches!(kind.kind.as_str(), "audio" | "video")
            {
                self.snapshot(&candidate)?;
            }
        }
        if let Some((bytes, inputs)) = self.load(&key, "hma")
            && let Ok(object) = AssetObject::decode(&bytes)
            && inputs.contains(&input_path(path)?)
            && object.validate_for(target).is_ok()
        {
            self.stats.reused_assets += 1;
            task.action("cache_hit");
            task.finish(true);
            self.report
                .note(hmi_media::describe_info(&object.metadata.info));
            return Ok(hmi_media::EncodingResult {
                action: "cache",
                object,
                inputs,
            });
        }
        let action = match kind.input.as_deref().unwrap_or("source") {
            "native" => "preserve_native",
            "encoded" => "package_encoded",
            _ if kind.original.is_some() => "process",
            _ => "encode",
        };
        task.action(action);
        self.report.note(format!("输入：{}", path.display()));
        let result = hmi_media::encode_with_progress(path, target, &task.progress_handle())
            .map_err(|e| task.error(e))?;
        self.report
            .note(hmi_media::describe_info(&result.object.metadata.info));
        task.action(match result.action {
            "original" => "reuse_original",
            "native" => "preserve_native",
            "encoded" => "package_encoded",
            _ => "encode",
        });
        self.store(&key, "hma", &result.object.encode()?, &result.inputs)?;
        self.stats.encoded += 1;
        task.finish(false);
        Ok(result)
    }

    pub fn encode_builtin(
        &mut self,
        id: hmi_core::builtin::BuiltinImage,
        target: &Target,
    ) -> Result<hmi_media::EncodingResult> {
        let key = self.key(
            "builtin",
            &self.root.join(id.id()),
            &(
                id.source_sha256(),
                &target.format,
                target.orientation,
                &target.capabilities.image_codecs,
            ),
        )?;
        if let Some((bytes, inputs)) = self.load(&key, "hma")
            && let Ok(object) = AssetObject::decode(&bytes)
            && inputs.is_empty()
            && object.validate_for(target).is_ok()
        {
            self.stats.reused_assets += 1;
            self.report
                .task(
                    "media",
                    Some("media"),
                    "cache_hit",
                    format!("内置图片 {}（构建缓存）", id.id()),
                )
                .finish(true);
            return Ok(hmi_media::EncodingResult {
                action: "cache",
                object,
                inputs,
            });
        }
        let task = self.report.task(
            "media",
            Some("media"),
            "encode",
            format!("内置图片 {}", id.id()),
        );
        let result = hmi_media::builtin::encode(id, target)?;
        self.store(&key, "hma", &result.object.encode()?, &[])?;
        self.stats.encoded += 1;
        task.finish(false);
        Ok(result)
    }

    pub fn protected_paths(&self) -> Vec<PathBuf> {
        self.owned
            .files
            .keys()
            .filter(|p| owned_path(p))
            .map(|p| self.root.join(p))
            .chain([self.root.join(LOCK), self.root.join(STATE)])
            .collect()
    }

    pub fn verify_inputs(&self) -> Result<()> {
        for (path, digest) in &self.snapshots {
            if sha256(&file::read(path)?) != *digest {
                return Err(file::at_file(
                    error("input_changed", "输入在构建期间发生变化，请重试"),
                    path,
                ));
            }
        }
        Ok(())
    }

    pub fn record(&mut self, paths: &[PathBuf]) -> Result<()> {
        for path in paths {
            let relative = path
                .strip_prefix(&self.root)
                .map_err(|_| error("build_directory", "构建产物必须位于工程内"))?
                .to_string_lossy()
                .replace('\\', "/");
            if !owned_path(&relative) {
                return Err(error("build_directory", "不能登记构建目录之外的清理项"));
            }
            self.owned
                .files
                .insert(relative, sha256(&file::read(path)?));
        }
        if self.deferred {
            return Ok(());
        }
        let state = self.root.join(STATE);
        self.protect(&state)?;
        file::write_atomic(
            &state,
            &file::json(&self.owned)?,
            &self
                .snapshots
                .keys()
                .map(PathBuf::as_path)
                .collect::<Vec<_>>(),
        )
    }
}

fn owned_path(relative: &str) -> bool {
    let path = Path::new(relative);
    if path
        .components()
        .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return false;
    }
    path.starts_with(DIRECTORY) || path.starts_with(crate::BUILD_DIRECTORY)
}

fn input_path(path: &Path) -> Result<PathBuf> {
    // DLL 加载器返回 Windows 长路径；与普通工程路径统一后比较同一文件。
    fs::canonicalize(path).map_err(|e| file::at_file(e.into(), path))
}

/// 只删除登记过且内容未被修改的派生文件；不递归删除目录，不接触 dist 或原始附件。
pub fn clean(root: &Path) -> Result<usize> {
    clean_with_report(root, &hmi_core::report::Report::default())
}

pub fn clean_with_report(root: &Path, report: &hmi_core::report::Report) -> Result<usize> {
    let root = fs::canonicalize(root)?;
    let inputs = declared_inputs(&root)?;
    let _lock = ProjectLock::acquire(&root, &inputs)?;
    safe_directory(&root, Path::new(DIRECTORY))?;
    safe_directory(&root, Path::new(crate::BUILD_DIRECTORY))?;
    let state_path = root.join(STATE);
    let protected: Vec<_> = inputs.iter().map(PathBuf::as_path).collect();
    file::protect_output(&state_path, &protected)?;
    let owned: Ownership = match file::read_json(&state_path) {
        Ok(owned) => owned,
        Err(_) if !state_path.exists() => return Ok(0),
        Err(e) => return Err(e),
    };
    let mut paths = Vec::new();
    let mut retained = Ownership::default();
    for (relative, digest) in owned.files {
        if !owned_path(&relative) {
            return Err(error("clean_path", "清理记录包含构建目录之外的路径"));
        }
        let path = root.join(&relative);
        safe_directory(&root, path.parent().unwrap().strip_prefix(&root).unwrap())?;
        file::protect_output(&path, &protected)?;
        if let Ok(metadata) = fs::symlink_metadata(&path) {
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                return Err(error(
                    "clean_path",
                    "已登记的构建产物被替换为目录或链接，停止清理",
                ));
            }
            if sha256(&file::read(&path)?) == digest {
                paths.push(path);
            } else {
                report.retained(&path);
                report.note(format!("保留已修改文件：{}", path.display()));
                retained.files.insert(relative, digest);
            }
        }
    }
    // 所有路径先通过检查，再开始删除；修改过的产物和未登记文件保留。
    for path in &paths {
        fs::remove_file(path).map_err(|e| file::at_file(e.into(), path))?;
        report.removed(path);
        report.note(format!("删除：{}", path.display()));
    }
    file::write_atomic(&state_path, &file::json(&retained)?, &protected)?;
    for relative in [
        ".build/hmi-toolchain/obj",
        ".build/hmi-toolchain/assets",
        crate::BUILD_DIRECTORY,
        DIRECTORY,
    ] {
        let path = root.join(relative);
        if path.is_dir() && fs::read_dir(&path)?.next().is_none() {
            fs::remove_dir(path)?;
        }
    }
    Ok(paths.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn target() -> Target {
        Target::resolve(&hmi_core::target::TargetConfig {
            profile: "TJC4827X543_011".into(),
            format: "ver21234".into(),
            encoding: "utf-8".into(),
            orientation: 0,
        })
        .unwrap()
    }
    #[test]
    fn builtin_cache_checks_payload_and_has_no_file_dependencies() {
        let tmp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(tmp.path()).unwrap();
        let id = hmi_core::builtin::BuiltinImage::BrowserFolder;
        let mut first = Cache::new(&root, false, &[]).unwrap();
        let expected = first
            .encode_builtin(id, &target())
            .unwrap()
            .object
            .encode()
            .unwrap();
        assert_eq!(first.stats.encoded, 1);
        let mut second = Cache::new(&root, false, &[]).unwrap();
        assert_eq!(
            second
                .encode_builtin(id, &target())
                .unwrap()
                .object
                .encode()
                .unwrap(),
            expected
        );
        assert_eq!(second.stats.reused_assets, 1);
        let artifact = fs::read_dir(root.join(DIRECTORY))
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| p.extension().is_some_and(|e| e == "hma"))
            .unwrap();
        fs::write(artifact, b"corrupt").unwrap();
        let mut third = Cache::new(&root, false, &[]).unwrap();
        assert_eq!(
            third
                .encode_builtin(id, &target())
                .unwrap()
                .object
                .encode()
                .unwrap(),
            expected
        );
        assert_eq!(third.stats.encoded, 1);
    }
    #[test]
    fn changed_input_is_not_entered_into_the_cache() {
        let tmp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(tmp.path()).unwrap();
        let path = root.join("program.s");
        fs::write(&path, "sys0=1\n").unwrap();
        let mut cache = Cache::new(&root, false, std::slice::from_ref(&path)).unwrap();
        fs::write(&path, "sys0=2\n").unwrap();
        let error = cache
            .compile(&path, &target(), None, None, true)
            .err()
            .unwrap();
        assert_eq!(error.as_diagnostic().unwrap().code, "input_changed");
        assert!(!root.join(DIRECTORY).exists());
    }
}
