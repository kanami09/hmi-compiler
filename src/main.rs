use clap::{Parser, Subcommand};
use hmi_compiler::{
    Result,
    container::Container,
    diagnostic::{Diagnostic, Location, Severity, has_errors},
    diff, project, workspace,
};
use serde_json::{Value, json};
use std::path::PathBuf;

#[derive(Parser)]
#[command(version, about = "陶晶驰 HMI 工程解析与重构建")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// 查看二进制工程结构和校验结果
    Inspect {
        file: PathBuf,
        #[arg(long)]
        pages: bool,
        #[arg(long)]
        resources: bool,
        #[arg(long)]
        json: bool,
    },
    /// 导出可读项目；保留完整原始数据以便独立回编
    Export {
        file: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(long)]
        force: bool,
        #[arg(long)]
        json: bool,
    },
    /// 检查可读项目及脚本辅助诊断，不输出工程文件
    Check {
        directory: PathBuf,
        #[arg(long)]
        deny_warnings: bool,
        #[arg(long)]
        json: bool,
    },
    /// 校验项目并回编为 .HMI
    Build {
        directory: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(long)]
        force: bool,
        #[arg(long)]
        deny_warnings: bool,
        #[arg(long)]
        json: bool,
    },
    /// 对比活动资源、属性和事件；发现差异仍返回成功
    Diff {
        old: PathBuf,
        new: PathBuf,
        #[arg(long)]
        json: bool,
    },
}

impl Command {
    fn json(&self) -> bool {
        match self {
            Self::Inspect { json, .. }
            | Self::Export { json, .. }
            | Self::Check { json, .. }
            | Self::Build { json, .. }
            | Self::Diff { json, .. } => *json,
        }
    }
    fn name(&self) -> &'static str {
        match self {
            Self::Inspect { .. } => "inspect",
            Self::Export { .. } => "export",
            Self::Check { .. } => "check",
            Self::Build { .. } => "build",
            Self::Diff { .. } => "diff",
        }
    }
    fn file(&self) -> String {
        match self {
            Self::Inspect { file, .. } | Self::Export { file, .. } => file,
            Self::Check { directory, .. } | Self::Build { directory, .. } => directory,
            Self::Diff { old, .. } => old,
        }
        .display()
        .to_string()
    }
}

struct Outcome {
    result: Value,
    diagnostics: Vec<Diagnostic>,
    ok: bool,
}

fn run(command: &Command) -> Result<Outcome> {
    let (result, mut diagnostics, deny_warnings) = match command {
        Command::Inspect {
            file,
            pages,
            resources,
            ..
        } => {
            let container = Container::parse(std::fs::read(file)?)?;
            let (report, diagnostics) = project::inspect(&container, *pages, *resources);
            (json!(report), diagnostics, false)
        }
        Command::Export {
            file,
            output,
            force,
            ..
        } => (
            json!(workspace::export(file, output, *force)?),
            Vec::new(),
            false,
        ),
        Command::Check {
            directory,
            deny_warnings,
            ..
        } => {
            let (report, diagnostics) = workspace::check(directory, false)?;
            (json!(report), diagnostics, *deny_warnings)
        }
        Command::Build {
            directory,
            output,
            force,
            deny_warnings,
            ..
        } => {
            if *deny_warnings {
                let (report, diagnostics) = workspace::check(directory, false)?;
                if !diagnostics.is_empty() {
                    let mut diagnostics = diagnostics;
                    diagnostics.push(Diagnostic::error(
                        "deny_warnings",
                        "存在警告，未写入构建输出",
                    ));
                    return Ok(Outcome {
                        result: json!(report),
                        diagnostics,
                        ok: false,
                    });
                }
            }
            let (report, diagnostics) =
                workspace::build(directory, output, *force, *deny_warnings)?;
            (json!(report), diagnostics, false)
        }
        Command::Diff { old, new, .. } => (json!(diff::diff(old, new)?), Vec::new(), false),
    };
    if deny_warnings && diagnostics.iter().any(|d| d.severity == Severity::Warning) {
        diagnostics.push(Diagnostic::error(
            "deny_warnings",
            "--deny-warnings：检查存在警告",
        ));
    }
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
    for d in &outcome.diagnostics {
        let label = if d.severity == Severity::Error {
            "错误"
        } else {
            "警告"
        };
        let location = &d.location;
        let mut at = location.file.clone().unwrap_or_default();
        if let Some(line) = location.line {
            at.push_str(&format!(":{line}"));
        }
        for text in [&location.page, &location.object, &location.property]
            .into_iter()
            .flatten()
        {
            at.push_str(&format!(" [{text}]"));
        }
        println!("{label} [{}] {at}：{}", d.code, d.message);
    }
}

fn main() {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            let status = error.exit_code();
            if status != 0 && std::env::args_os().any(|a| a == "--json") {
                println!(
                    "{}",
                    json!({"schema_version":1,"ok":false,"result":null,"diagnostics":[Diagnostic::error("arguments",error.to_string())]})
                );
            } else {
                let _ = error.print();
            }
            std::process::exit(status);
        }
    };
    let outcome = match run(&cli.command) {
        Ok(outcome) => outcome,
        Err(error) => Outcome {
            result: Value::Null,
            diagnostics: vec![error.as_diagnostic().cloned().unwrap_or_else(|| {
                Diagnostic::error("execution", error.to_string()).at(Location {
                    file: Some(cli.command.file()),
                    ..Location::default()
                })
            })],
            ok: false,
        },
    };
    if cli.command.json() {
        println!(
            "{}",
            json!({"schema_version":1,"ok":outcome.ok,"result":outcome.result,"diagnostics":outcome.diagnostics})
        );
    } else {
        human(cli.command.name(), &outcome);
    }
    if !outcome.ok {
        std::process::exit(1);
    }
}
