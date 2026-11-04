mod output;
use clap::{Args, Parser, Subcommand};
use hmi_compiler::{
    Result,
    build::{self, BuildMode, BuildOptions},
    container::Container,
    diagnostic::{Diagnostic, Location, has_errors},
    diff, project,
    warning::{CATEGORIES, WarningPolicy},
    workspace,
};
use output::{ColorMode, Renderer};
use serde_json::{Value, json};
use std::path::PathBuf;

fn warning_help() -> String {
    let mut text = String::from(
        "警告控制：-W类别 / -Wno-类别 / -Werror / -Wno-error / -Werror=类别 / -Wno-error=类别 / -Wall / -Wno-all / -w\n支持的类别：\n",
    );
    for (name, description) in CATEGORIES {
        text.push_str(&format!("  {name:<20} {description}\n"));
    }
    text
}

#[derive(Parser)]
#[command(version, about = "陶晶驰 HMI 工程解析与重构建")]
struct Cli {
    #[arg(
        long,
        global = true,
        value_enum,
        default_value = "auto",
        help = "终端颜色策略"
    )]
    color: ColorMode,
    #[command(subcommand)]
    command: Command,
}

#[derive(Args, Clone, Default)]
struct DiagnosticArgs {
    #[arg(short = 'W', action = clap::ArgAction::Append, value_name = "OPTION", help = "按类别控制警告，可重复，如 -Werror -Wno-script-dynamic")]
    warnings: Vec<String>,
    #[arg(short = 'w', help = "抑制全部警告")]
    suppress: bool,
    #[arg(long, help = "输出单个 JSON 对象")]
    json: bool,
}
#[derive(Args)]
#[command(after_help = warning_help())]
struct InspectArgs {
    file: PathBuf,
    #[arg(long)]
    pages: bool,
    #[arg(long)]
    resources: bool,
    #[command(flatten)]
    diagnostics: DiagnosticArgs,
}
#[derive(Args)]
#[command(after_help = warning_help())]
struct ExportArgs {
    file: PathBuf,
    #[arg(short, long)]
    output: PathBuf,
    #[arg(long)]
    force: bool,
    #[command(flatten)]
    diagnostics: DiagnosticArgs,
}
#[derive(Args)]
#[command(after_help = warning_help())]
struct CheckArgs {
    directory: PathBuf,
    #[command(flatten)]
    diagnostics: DiagnosticArgs,
}
#[derive(Args)]
#[command(after_help = warning_help())]
struct BuildArgs {
    directory: PathBuf,
    #[arg(short, long)]
    output: PathBuf,
    #[command(flatten)]
    diagnostics: DiagnosticArgs,
}
#[derive(Args)]
#[command(after_help = warning_help())]
struct DiffArgs {
    old: PathBuf,
    new: PathBuf,
    #[command(flatten)]
    diagnostics: DiagnosticArgs,
}

#[derive(Subcommand)]
enum Command {
    /// 查看工程结构与校验结果
    Inspect(InspectArgs),
    /// 导出可读项目，--force 允许重新生成工具管理的文件
    Export(ExportArgs),
    /// 检查工程，不写目标文件或构建缓存
    Check(CheckArgs),
    /// 增量构建工程，成功后更新目标文件
    Build(BuildArgs),
    /// 全量重建工程，参数与 build 相同
    Rebuild(BuildArgs),
    /// 对比两个工程的活动资源与语义变化
    Diff(DiffArgs),
}
impl Command {
    fn diagnostics(&self) -> &DiagnosticArgs {
        match self {
            Self::Inspect(a) => &a.diagnostics,
            Self::Export(a) => &a.diagnostics,
            Self::Check(a) => &a.diagnostics,
            Self::Build(a) | Self::Rebuild(a) => &a.diagnostics,
            Self::Diff(a) => &a.diagnostics,
        }
    }
    fn name(&self) -> &'static str {
        match self {
            Self::Inspect(_) => "inspect",
            Self::Export(_) => "export",
            Self::Check(_) => "check",
            Self::Build(_) => "build",
            Self::Rebuild(_) => "rebuild",
            Self::Diff(_) => "diff",
        }
    }
    fn file(&self) -> String {
        match self {
            Self::Inspect(a) => &a.file,
            Self::Export(a) => &a.file,
            Self::Check(a) => &a.directory,
            Self::Build(a) | Self::Rebuild(a) => &a.directory,
            Self::Diff(a) => &a.old,
        }
        .display()
        .to_string()
    }
    fn engine(&self) -> bool {
        matches!(self, Self::Build(_) | Self::Rebuild(_) | Self::Check(_))
    }
}
struct Outcome {
    result: Value,
    diagnostics: Vec<Diagnostic>,
    ok: bool,
}

fn run(command: &Command, policy: &WarningPolicy, renderer: &mut Renderer) -> Result<Outcome> {
    let quiet = command.diagnostics().json;
    let mut observer = |event: &build::BuildEvent| {
        if !quiet {
            renderer.event(event);
        }
    };
    let (result, raw) = match command {
        Command::Build(args) | Command::Rebuild(args) => {
            let mode = if matches!(command, Command::Rebuild(_)) {
                BuildMode::Full
            } else {
                BuildMode::Incremental
            };
            let outcome = build::run(
                &args.directory,
                Some(&args.output),
                &BuildOptions {
                    mode,
                    warnings: policy.clone(),
                },
                &mut observer,
            );
            return Ok(Outcome {
                result: json!(outcome.report),
                diagnostics: outcome.diagnostics,
                ok: outcome.success,
            });
        }
        Command::Check(args) => {
            let outcome = build::run(
                &args.directory,
                None,
                &BuildOptions {
                    mode: BuildMode::Check,
                    warnings: policy.clone(),
                },
                &mut observer,
            );
            return Ok(Outcome {
                result: json!(outcome.report),
                diagnostics: outcome.diagnostics,
                ok: outcome.success,
            });
        }
        Command::Inspect(args) => {
            let container = Container::parse(std::fs::read(&args.file)?)?;
            let (report, diagnostics) = project::inspect(&container, args.pages, args.resources);
            (json!(report), diagnostics)
        }
        Command::Export(args) => (
            json!(workspace::export(&args.file, &args.output, args.force)?),
            Vec::new(),
        ),
        Command::Diff(args) => (json!(diff::diff(&args.old, &args.new)?), Vec::new()),
    };
    let diagnostics = policy.filter(&raw);
    let ok = !has_errors(&diagnostics);
    Ok(Outcome {
        result,
        diagnostics,
        ok,
    })
}

fn brief(value: &Value) -> String {
    let text = value.to_string();
    if text.chars().count() > 180 {
        format!("{}…", text.chars().take(180).collect::<String>())
    } else {
        text
    }
}

fn human(command: &str, outcome: &Outcome) {
    let r = &outcome.result;
    if !r.is_null() {
        match command {
            "inspect" => {
                println!(
                    "版本 {}，{} 字节，{} 个页面，{} 个对象，{} 个事件槽",
                    r["version"].as_str().unwrap_or("?"),
                    r["file_size"],
                    r["pages"],
                    r["objects"],
                    r["events"]
                );
                println!(
                    "资源：{} 活动 / {} 已删除；连续布局：{}",
                    r["active_resources"], r["deleted_resources"], r["contiguous"]
                );
                for (label, key) in [
                    ("主目录", "primary_directory"),
                    ("备份目录", "backup_directory"),
                ] {
                    if let Some(crc) = r[key]["stored_crc"].as_u64() {
                        println!(
                            "{label}：{} 条，CRC 0x{crc:08X}，校验={}",
                            r[key]["count"], r[key]["crc_valid"]
                        );
                    }
                }
                if let Some(pages) = r["page_list"].as_array() {
                    for p in pages {
                        println!(
                            "页 {}：{} [{}]，{} 对象",
                            p["id"],
                            p["name"].as_str().unwrap_or("?"),
                            p["resource"].as_str().unwrap_or("?"),
                            p["objects"]
                        );
                    }
                }
                if let Some(resources) = r["resource_list"].as_array() {
                    for e in resources {
                        println!(
                            "资源 #{}：{}，0x{:X}，{} 字节，删除={}",
                            e["index"],
                            e["name"].as_str().unwrap_or("?"),
                            e["offset"].as_u64().unwrap_or(0),
                            e["length"],
                            e["deleted"]
                        );
                    }
                }
            }
            "export" => println!(
                "已导出到 {}：{} 个页面，{} 个文件",
                r["output"].as_str().unwrap_or("?"),
                r["pages"],
                r["files"]
            ),
            "check" => println!(
                "项目检查{}；无修改往返={}；变化资源={}",
                if outcome.ok { "通过" } else { "未通过" },
                r["byte_identical"],
                r["changed_resources"]
            ),
            "build" => {
                if let Some(output) = r["output"].as_str() {
                    println!(
                        "已构建 {output}：{} 字节；无修改往返={}；变化资源={}",
                        r["size"], r["byte_identical"], r["changed_resources"]
                    );
                }
            }
            "diff" => {
                println!(
                    "逐字节相同={}；活动资源相同={}；容器发生变化={}",
                    r["byte_identical"], r["active_resources_equal"], r["container_changed"]
                );
                if let Some(changes) = r["changes"].as_array() {
                    for c in changes {
                        println!(
                            "{} {} {} {} {}：{} → {}",
                            c["kind"].as_str().unwrap_or("?"),
                            c["resource"].as_str().unwrap_or(""),
                            c["page"].as_str().unwrap_or(""),
                            c["object"].as_str().unwrap_or(""),
                            c["field"].as_str().unwrap_or(""),
                            brief(&c["before"]),
                            brief(&c["after"])
                        );
                    }
                }
            }
            _ => {}
        }
    }
}

fn emit_json(outcome: &Outcome) {
    println!(
        "{}",
        json!({"schema_version":1,"ok":outcome.ok,"result":outcome.result,"diagnostics":outcome.diagnostics})
    );
}

fn main() {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            let status = error.exit_code();
            if status != 0 && std::env::args_os().any(|a| a == "--json") {
                emit_json(&Outcome {
                    result: Value::Null,
                    ok: false,
                    diagnostics: vec![Diagnostic::error("arguments", error.to_string())],
                });
            } else {
                let _ = error.print();
            }
            std::process::exit(status);
        }
    };
    let args = cli.command.diagnostics();
    let mut renderer = Renderer::new(cli.color);
    let policy = match WarningPolicy::parse(&args.warnings, args.suppress) {
        Ok(policy) => policy,
        Err(error) => {
            let outcome = Outcome {
                result: Value::Null,
                ok: false,
                diagnostics: vec![Diagnostic::error("arguments", error.to_string())],
            };
            if args.json {
                emit_json(&outcome);
            } else {
                renderer.diagnostic(&outcome.diagnostics[0], None);
            }
            std::process::exit(2);
        }
    };
    let outcome = match run(&cli.command, &policy, &mut renderer) {
        Ok(outcome) => outcome,
        Err(error) => Outcome {
            result: Value::Null,
            ok: false,
            diagnostics: vec![error.as_diagnostic().cloned().unwrap_or_else(|| {
                Diagnostic::error("execution", error.to_string()).at(Location {
                    file: Some(cli.command.file()),
                    ..Location::default()
                })
            })],
        },
    };
    if args.json {
        emit_json(&outcome);
    } else if !cli.command.engine() {
        human(cli.command.name(), &outcome);
        for diagnostic in &outcome.diagnostics {
            renderer.diagnostic(diagnostic, None);
        }
    }
    if !outcome.ok {
        std::process::exit(1);
    }
}
