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
    #[arg(long, global = true)]
    json: bool,
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
    },
    /// 执行全量重建。
    Rebuild {
        directory: Option<PathBuf>,
        #[arg(short, long)]
        output: Option<PathBuf>,
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
fn run(args: &Args) -> Result<CommandResult> {
    match &args.command {
        Command::Import { file, output } => Ok(cli::success(
            serde_json::json!(hmi_parser::export(file, output)?),
            format!("新版工程已导入：{}", output.display()),
        )),
        Command::Inspect { file } => Ok(cli::success(
            hmi_parser::inspect(file)?,
            "HMI结构检查通过。",
        )),
        Command::Init {
            directory,
            target: profile,
            orientation,
        } => {
            hmi_cli::init(directory, &target(profile, *orientation))?;
            Ok(cli::success(
                serde_json::json!({"directory":directory}),
                format!("空白工程已创建：{}", directory.display()),
            ))
        }
        Command::Check { directory } => {
            let root = hmi_cli::locate_root(directory.as_deref())?;
            let prepared = hmi_cli::prepare(&root)?;
            let diagnostics = prepared.check(args.warnings.is_some())?;
            Ok(CommandResult {
                result: serde_json::json!({"pages":prepared.pages.len(),"output_written":false}),
                message: "工程检查通过。".into(),
                diagnostics,
            })
        }
        Command::Build { directory, output } | Command::Rebuild { directory, output } => {
            let root = hmi_cli::locate_root(directory.as_deref())?;
            let force = matches!(args.command, Command::Rebuild { .. });
            let (output, linked, stats) = hmi_cli::build_with_options(
                &root,
                output.as_deref(),
                args.warnings.is_some(),
                force,
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
            let removed = hmi_cli::clean(&root)?;
            Ok(cli::success(
                serde_json::json!({"removed":removed}),
                format!("清理完成，删除 {removed} 个构建产物。"),
            ))
        }
        Command::Plan { directory, output } => {
            let root = hmi_cli::locate_root(directory.as_deref())?;
            hmi_cli::prepare(&root)?.save_plan(output, false, None)?;
            Ok(cli::success(
                serde_json::json!({"directory":output}),
                format!("构建计划已导出：{}", output.display()),
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
                let target = Target::resolve(&target(profile, *orientation))?;
                file::write_atomic(output, &file::json(&target)?, &[])?;
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
    cli::finish(run(&args), args.json)
}
