//! 工程初始化、执行计划和完整构建。
//!
//! 编译和链接直接调用共享库；计划中的独立命令使用相同的输入与格式。

use hmi_core::{
    Result, error, file,
    model::*,
    target::{DEFAULT_PROGRAM, Target, TargetConfig},
};
use hmi_page_compiler::{Compilation, compile_page, compile_program};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

pub const BUILD_DIRECTORY: &str = ".build/hmi-toolchain";

pub struct Prepared {
    pub root: PathBuf,
    pub project: ProjectSource,
    pub target: Target,
    pub program: Compilation,
    pub pages: Vec<Compilation>,
    pub inputs: Vec<PathBuf>,
}

/// 显式目录直接使用；未指定时向上查找工程清单。
pub fn locate_root(directory: Option<&Path>) -> Result<PathBuf> {
    if let Some(directory) = directory {
        return fs::canonicalize(directory).map_err(|e| file::at_file(e.into(), directory));
    }
    let current = std::env::current_dir()?;
    current
        .ancestors()
        .find(|dir| dir.join("hmi.toml").is_file())
        .map(Path::to_path_buf)
        .ok_or_else(|| error("project_missing", "当前目录及其父目录中没有 hmi.toml"))
}

pub fn init(directory: &Path, config: &TargetConfig) -> Result<()> {
    Target::resolve(config)?;
    if directory.exists() && fs::read_dir(directory)?.next().is_some() {
        return Err(error(
            "init_directory",
            "工程目录必须不存在或为空，已有文件不会被覆盖",
        ));
    }
    let project = ProjectSource {
        schema: "hmi.project/1".into(),
        name: "Panel".into(),
        target: config.clone(),
        program: ProgramSource {
            source: "program.s".into(),
        },
        build: BuildConfig::default(),
        pages: PageSources {
            sources: vec!["pages/page0/page.toml".into()],
        },
        assets: AssetSources::default(),
    };
    let files = [
        ("hmi.toml", file::toml(&project)?),
        ("program.s", DEFAULT_PROGRAM.as_bytes().to_vec()),
        (
            "pages/page0/page.toml",
            file::toml(&hmi_page_compiler::empty_page("page0"))?,
        ),
    ];
    // 使用 create_new，目录检查后的并发文件创建也不能导致原文件被覆盖。
    for (relative, bytes) in files {
        let path = directory.join(relative);
        fs::create_dir_all(path.parent().unwrap())?;
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| file::at_file(e.into(), &path))?;
        output
            .write_all(&bytes)
            .map_err(|e| file::at_file(e.into(), &path))?;
        output.sync_all()?;
    }
    Ok(())
}

pub fn prepare(root: &Path) -> Result<Prepared> {
    let root = fs::canonicalize(root)?;
    let manifest = root.join("hmi.toml");
    let project: ProjectSource = file::read_toml(&manifest)?;
    check_schema(&project.schema, "hmi.project/1")?;
    if project.name.trim().is_empty() {
        return Err(error("project_name", "工程名称不能为空"));
    }
    if project.pages.sources.is_empty() {
        return Err(error("page_count", "工程至少需要一个页面"));
    }
    if !project.assets.items.is_empty() {
        return Err(error(
            "project_assets",
            "当前阶段尚未接入媒体资产，无法处理含资产的工程",
        ));
    }
    let target = Target::resolve(&project.target)?;
    let program = compile_program(&root.join(&project.program.source), &target)?;
    let mut inputs = vec![manifest];
    inputs.extend(program.inputs.iter().cloned());
    let mut pages = Vec::new();
    let mut paths = std::collections::BTreeSet::new();
    for source in &project.pages.sources {
        let path = root.join(source);
        let canonical = fs::canonicalize(&path).map_err(|e| file::at_file(e.into(), &path))?;
        if !paths.insert(canonical) {
            return Err(error("page_source", format!("页面输入 {source} 重复")));
        }
        let compilation = compile_page(&path, &target)?;
        inputs.extend(compilation.inputs.iter().cloned());
        pages.push(compilation);
    }
    Ok(Prepared {
        root,
        project,
        target,
        program,
        pages,
        inputs,
    })
}

impl Prepared {
    pub fn link(&self, deny_warnings: bool) -> Result<hmi_linker::Linked> {
        hmi_linker::link(
            &self.target,
            &self.program.object,
            &self
                .pages
                .iter()
                .map(|p| p.object.clone())
                .collect::<Vec<_>>(),
            deny_warnings,
        )
    }

    pub fn output_path(&self, requested: Option<&Path>) -> Result<PathBuf> {
        if let Some(path) = requested {
            return file::absolute(path);
        }
        if let Some(path) = &self.project.build.output {
            return Ok(self.root.join(path));
        }
        let name = &self.project.name;
        if name.contains(['/', '\\', ':', '\0', '<', '>', '"', '|', '?', '*'])
            || matches!(name.as_str(), "." | "..")
        {
            return Err(error(
                "output_path",
                "工程名称不能用作默认输出路径，请在 build.output 中指定合法路径",
            ));
        }
        Ok(self
            .root
            .join("dist")
            .join(name)
            .join(format!("{name}.HMI")))
    }

    fn plan_paths(&self, directory: &Path) -> Vec<PathBuf> {
        let mut paths = vec![
            directory.join("target.json"),
            directory.join("link.toml"),
            directory.join("jobs.json"),
            directory.join("obj/program.hmo"),
        ];
        paths.extend((0..self.pages.len()).map(|id| directory.join(format!("obj/p{id:05}.hmo"))));
        paths
    }

    /// 计划是显式的工具调用参数，不包含可执行的 shell 字符串。
    pub fn save_plan(
        &self,
        directory: &Path,
        write_objects: bool,
        requested_output: Option<&Path>,
    ) -> Result<()> {
        let directory = file::absolute(directory)?;
        let protected: Vec<_> = self.inputs.iter().map(|p| p.as_path()).collect();
        let paths = self.plan_paths(&directory);
        for path in &paths {
            file::protect_output(path, &protected)?;
        }
        let target = directory.join("target.json");
        let program = directory.join("obj/program.hmo");
        let manifest = LinkSource {
            schema: "hmi.link/1".into(),
            name: self.project.name.clone(),
            target: "target.json".into(),
            program: "obj/program.hmo".into(),
            pages: PageObjects {
                objects: (0..self.pages.len())
                    .map(|id| format!("obj/p{id:05}.hmo"))
                    .collect(),
            },
            assets: AssetObjects::default(),
        };
        let mut jobs = vec![
            serde_json::json!({"command":"hmi-compile","args":["program",self.root.join(&self.project.program.source),"--target",target,"-o",program]}),
        ];
        for (id, source) in self.project.pages.sources.iter().enumerate() {
            let output = directory.join(format!("obj/p{id:05}.hmo"));
            jobs.push(serde_json::json!({"command":"hmi-compile","args":["page",self.root.join(source),"--target",target,"-o",output]}));
            if write_objects {
                self.pages[id].object.write(&output, &protected)?;
            }
        }
        let output = self.output_path(requested_output)?;
        jobs.push(serde_json::json!({"command":"hmi-link","args":[directory.join("link.toml"),"-o",output]}));
        file::write_atomic(&target, &file::json(&self.target)?, &protected)?;
        file::write_atomic(
            &directory.join("link.toml"),
            &file::toml(&manifest)?,
            &protected,
        )?;
        file::write_atomic(
            &directory.join("jobs.json"),
            &file::json(&serde_json::json!({"schema":"hmi.build-plan/1","jobs":jobs}))?,
            &protected,
        )?;
        if write_objects {
            self.program.object.write(&program, &protected)?;
        }
        Ok(())
    }
}

/// 第一阶段没有增量缓存：所有阶段完成并通过检查后才更新最终 HMI。
pub fn build(
    root: &Path,
    requested: Option<&Path>,
    deny_warnings: bool,
) -> Result<(PathBuf, hmi_linker::Linked)> {
    let prepared = prepare(root)?;
    let linked = prepared.link(deny_warnings)?;
    let output = prepared.output_path(requested)?;
    let map = output.parent().unwrap().join("link-map.json");
    if file::absolute(&output)? == file::absolute(&map)? {
        return Err(error("output_path", "HMI 与链接映射不能使用同一输出路径"));
    }
    let build_directory = prepared.root.join(BUILD_DIRECTORY);
    let mut protected_paths = prepared.inputs.clone();
    protected_paths.extend(prepared.plan_paths(&build_directory));
    let protected: Vec<_> = protected_paths.iter().map(|p| p.as_path()).collect();
    file::protect_output(&output, &protected)?;
    file::protect_output(&map, &protected)?;
    prepared.save_plan(&build_directory, true, Some(&output))?;
    file::write_atomic(&map, &file::json(&linked.map)?, &protected)?;
    file::write_atomic(&output, &linked.bytes, &protected)?;
    Ok((output, linked))
}
