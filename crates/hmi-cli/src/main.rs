use clap::{Parser, Subcommand};
use hmi_core::{
    Result,
    cli::{self, CommandResult},
    file,
    target::{PROFILES, Target, TargetConfig},
};
use std::{path::PathBuf, process::ExitCode};

#[derive(Parser)]
#[command(name = "hmi", version, about = "HMI 工程管理与构建")]
struct Args {
    #[command(flatten)]
    reporting: cli::OutputOptions,
    #[arg(short='W', global=true, value_parser=["error"], help="将警告作为错误：-Werror")]
    warnings: Option<String>,
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    /// 从已有HMI导入新版工程，包含已识别的五类媒体。
    Import {
        file: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// 查看已有HMI的目标、页面与活动资源。
    Inspect { file: PathBuf },
    /// 新建不含字体或图片的空白工程。
    Init {
        directory: PathBuf,
        #[arg(long)]
        target: String,
        #[arg(long, default_value_t = 0)]
        orientation: u16,
    },
    /// 检查工程，不写目标文件或构建目录。
    Check { directory: Option<PathBuf> },
    /// 按内容摘要复用编译和编码产物，构建工程。
    Build {
        directory: Option<PathBuf>,
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[arg(short='j',long,value_parser=clap::value_parser!(u16).range(1..=256),help="并行任务上限；默认按CPU数量、最多8个，媒体最多2个")]
        jobs: Option<u16>,
    },
    /// 执行全量重建。
    Rebuild {
        directory: Option<PathBuf>,
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[arg(short='j',long,value_parser=clap::value_parser!(u16).range(1..=256),help="并行任务上限；默认按CPU数量、最多8个，媒体最多2个")]
        jobs: Option<u16>,
    },
    /// 清理已登记且未被修改的构建产物，保留源文件和最终输出。
    Clean { directory: Option<PathBuf> },
    /// 导出独立工具执行计划，不生成 .hmo 或 HMI。
    Plan {
        directory: Option<PathBuf>,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// 查看或导出当前已支持的目标配置。
    Target {
        #[command(subcommand)]
        action: TargetAction,
    },
}
#[derive(Subcommand)]
enum TargetAction {
    List,
    Export {
        profile: String,
        #[arg(long, default_value_t = 0)]
        orientation: u16,
        #[arg(short, long)]
        output: PathBuf,
    },
    Show {
        profile: String,
        #[arg(long, default_value_t = 0)]
        orientation: u16,
    },
}
fn target(profile: &str, orientation: u16) -> TargetConfig {
    TargetConfig {
        profile: profile.into(),
        format: "ver21234".into(),
        encoding: "utf-8".into(),
        orientation,
    }
}
fn run(args: &Args, session: &cli::Session) -> Result<CommandResult> {
    match &args.command {
        Command::Import { file, output } => Ok(cli::success(
            serde_json::json!(hmi_parser::export_with_report(
                file,
                output,
                &session.report
            )?),
            format!("新版工程已导入：{}", output.display()),
        )),
        Command::Inspect { file } => {
            let task = session.report.task(
                "inspect",
                None,
                "validate",
                format!("HMI {}", file.display()),
            );
            let result = hmi_parser::inspect(file).map_err(|e| task.error(e))?;
            task.finish(false);
            Ok(cli::success(result, "HMI结构检查通过。"))
        }
        Command::Init {
            directory,
            target: profile,
            orientation,
        } => {
            hmi_cli::init_with_report(directory, &target(profile, *orientation), &session.report)?;
            Ok(cli::success(
                serde_json::json!({"directory":directory}),
                format!("空白工程已创建：{}", directory.display()),
            ))
        }
        Command::Check { directory } => {
            let root = hmi_cli::locate_root(directory.as_deref())?;
            session.report.stage("load", "check", "工程配置与输入文件");
            let prepared = hmi_cli::prepare_with_report(&root, &session.report)?;
            let checking = session
                .report
                .task("analysis", None, "check", "页面、脚本及资源引用");
            let diagnostics = prepared.check(args.warnings.is_some())?;
            checking.finish(false);
            Ok(CommandResult {
                result: serde_json::json!({"pages":prepared.pages.len(),"output_written":false}),
                message: prepared.project.name.clone(),
                diagnostics,
            })
        }
        Command::Build {
            directory,
            output,
            jobs,
        }
        | Command::Rebuild {
            directory,
            output,
            jobs,
        } => {
            let root = hmi_cli::locate_root(directory.as_deref())?;
            let force = matches!(args.command, Command::Rebuild { .. });
            let (output, linked, stats) = hmi_cli::build_with_jobs(
                &root,
                output.as_deref(),
                args.warnings.is_some(),
                force,
                jobs.map(usize::from).unwrap_or_else(hmi_cli::default_jobs),
                &session.report,
            )?;
            Ok(CommandResult {
                result: serde_json::json!({"output":output,"output_written":true,"size":linked.bytes.len(),"sha256":hmi_core::sha256(&linked.bytes),"external_files":linked.external_files.iter().map(|f| &f.path).collect::<Vec<_>>(),"build":stats}),
                message: format!(
                    "重建完成：{}\n编译 {} 项，复用目标 {} 项；处理媒体 {} 项，复用媒体 {} 项。",
                    output.display(),
                    stats.compiled,
                    stats.reused_objects,
                    stats.encoded,
                    stats.reused_assets
                ),
                diagnostics: linked.diagnostics,
            })
        }
        Command::Clean { directory } => {
            let root = hmi_cli::locate_root(directory.as_deref())?;
            let project: hmi_core::model::ProjectSource = file::read_toml(&root.join("hmi.toml"))?;
            session.report.project(&project.name, &root)?;
            let task = session.report.task(
                "clean",
                None,
                "clean",
                format!("{} 下已登记的构建产物", root.display()),
            );
            let removed =
                hmi_cli::clean_with_report(&root, &session.report).map_err(|e| task.error(e))?;
            task.finish(false);
            Ok(cli::success(
                serde_json::json!({"removed":removed}),
                format!(
                    "清理 {removed} 个构建产物，保留 {} 个已修改文件",
                    session.report.retained_count()
                ),
            ))
        }
        Command::Plan { directory, output } => {
            let root = hmi_cli::locate_root(directory.as_deref())?;
            session.report.stage("load", "check", "工程配置与输入文件");
            let prepared = hmi_cli::prepare_with_report(&root, &session.report)?;
            prepared.save_plan_with_report(output, None, &session.report)?;
            Ok(cli::success(
                serde_json::json!({"directory":output}),
                format!(
                    "构建计划已导出：{}（{} 项任务）",
                    output.display(),
                    prepared.pages.len() + prepared.assets.len() + 2
                ),
            ))
        }
        Command::Target { action } => match action {
            TargetAction::List => Ok(cli::success(
                serde_json::json!({"profiles":PROFILES}),
                PROFILES.join("\n"),
            )),
            TargetAction::Export {
                profile,
                orientation,
                output,
            } => {
                session.report.plan_output("target", output)?;
                let exporting =
                    session
                        .report
                        .task("export", None, "export", format!("目标配置 {profile}"));
                let target = Target::resolve(&target(profile, *orientation))?;
                file::write_atomic(output, &file::json(&target)?, &[])?;
                session
                    .report
                    .written("target", output, Some(&file::json(&target)?))?;
                exporting.finish(false);
                Ok(cli::success(
                    serde_json::json!({"output":output}),
                    format!("目标配置已导出：{}", output.display()),
                ))
            }
            TargetAction::Show {
                profile,
                orientation,
            } => {
                let target = Target::resolve(&target(profile, *orientation))?;
                Ok(cli::success(
                    serde_json::json!(target),
                    format!(
                        "{}，{}°，页面 {}×{}。",
                        profile, orientation, target.canvas.width, target.canvas.height
                    ),
                ))
            }
        },
    }
}
fn main() -> ExitCode {
    let args: Args = match cli::parse() {
        Ok(args) => args,
        Err(code) => return code,
    };
    let session = cli::Session::new(
        "hmi",
        env!("CARGO_PKG_VERSION"),
        &cli::command_name::<Args>(),
        &args.reporting,
    );
    let outcome = run(&args, &session);
    session.finish(outcome)
}
