use clap::{Parser, Subcommand};
use hmi_artifact::asset::AssetObject;
use hmi_core::{
    Result,
    cli::{self, CommandResult},
    file,
    target::Target,
};
use std::{path::PathBuf, process::ExitCode};

#[derive(Parser)]
#[command(
    name = "hmi-media",
    version,
    about = "处理 HMI 图片、字库、动画、视频和音频资产"
)]
struct Args {
    #[command(flatten)]
    reporting: cli::OutputOptions,
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    /// 检查可选 FFmpeg DLL 的版本和 ABI。
    Backend,
    /// 生成受控内置图标，供独立构建计划使用。
    Builtin {
        id: String,
        #[arg(long)]
        target: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// 检查媒体输入及处理参数，不写输出。
    Probe {
        source: PathBuf,
        #[arg(long)]
        target: PathBuf,
    },
    /// 编码媒体，生成自包含的 .hma。
    Encode {
        source: PathBuf,
        #[arg(long)]
        target: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// 校验并查看资产目标的描述和成员。
    Inspect { file: PathBuf },
}
fn timing_note(info: &hmi_artifact::asset::AssetInfo) -> String {
    format!(" {}。", hmi_media::describe_info(info))
}

fn run(args: &Args, session: &cli::Session) -> Result<CommandResult> {
    match &args.command {
        Command::Backend => {
            let task = session.report.task("media", None, "check", "FFmpeg 后端");
            let result = serde_json::json!({"directory":hmi_media::ffmpeg::directory()?,"backend":hmi_media::ffmpeg::backend_info()?});
            task.finish(false);
            Ok(cli::success(result, "FFmpeg 后端检查通过。"))
        }
        Command::Builtin { id, target, output } => {
            session.report.plan("media", 1);
            session.report.plan_output("asset_object", output)?;
            let task =
                session
                    .report
                    .task("media", Some("media"), "encode", format!("内置图片 {id}"));
            let builtin = hmi_core::builtin::BuiltinImage::from_id(id).ok_or_else(|| {
                hmi_core::error(
                    "builtin_asset",
                    "未知内置图标；可选browser-folder、browser-file",
                )
            })?;
            let config: Target = file::read_json(target)?;
            let encoded = hmi_media::builtin::encode(builtin, &config)?;
            encoded.object.write(output, &[target])?;
            session
                .report
                .written("asset_object", output, Some(&encoded.object.encode()?))?;
            task.finish(false);
            Ok(cli::success(
                serde_json::json!({"output":output,"asset":encoded.object.metadata.info,"input_action":"builtin","builtin":id}),
                format!("内置图标已生成：{}", output.display()),
            ))
        }
        Command::Inspect { file } => {
            let task = session.report.task(
                "inspect",
                None,
                "validate",
                format!("资产目标 {}", file.display()),
            );
            let asset = AssetObject::read(file)?;
            task.finish(false);
            Ok(cli::success(
                serde_json::json!({"metadata":asset.metadata,"payload_size":asset.payload.len()}),
                "资产目标校验通过。",
            ))
        }
        Command::Probe { source, target } => {
            let task =
                session
                    .report
                    .task("media", None, "check", format!("媒体 {}", source.display()));
            let target: Target = file::read_json(target)?;
            let probe = hmi_media::probe(source, &target)?;
            let message = format!("媒体输入检查通过。{}", timing_note(&probe.info));
            let mut value = serde_json::json!(probe.info);
            value["input_action"] = probe.action.into();
            task.finish(false);
            Ok(cli::success(value, message))
        }
        Command::Encode {
            source,
            target,
            output,
        } => {
            session.report.plan("media", 1);
            session.report.plan_output("asset_object", output)?;
            let task = session.report.task(
                "media",
                Some("media"),
                "process",
                format!("媒体 {}", source.display()),
            );
            session.report.note(format!("输入：{}", source.display()));
            let config: Target = file::read_json(target)?;
            let encoded = hmi_media::encode(source, &config)?;
            let mut inputs = encoded
                .inputs
                .iter()
                .map(|p| p.as_path())
                .collect::<Vec<_>>();
            inputs.push(target);
            encoded.object.write(output, &inputs)?;
            task.action(match encoded.action {
                "original" => "reuse_original",
                "native" => "preserve_native",
                "encoded" => "package_encoded",
                _ => "encode",
            });
            session
                .report
                .written("asset_object", output, Some(&encoded.object.encode()?))?;
            task.finish(false);
            let note = timing_note(&encoded.object.metadata.info);
            Ok(cli::success(
                serde_json::json!({"output":output,"asset":encoded.object.metadata.info,"input_action":encoded.action}),
                format!(
                    "{}：{}{}",
                    match encoded.action {
                        "original" => "原始编码已复用",
                        "native" => "原生资产包已保留",
                        "encoded" => "编码文件已封装",
                        _ => "媒体编码完成",
                    },
                    output.display(),
                    note
                ),
            ))
        }
    }
}
fn main() -> ExitCode {
    let args: Args = match cli::parse() {
        Ok(a) => a,
        Err(code) => return code,
    };
    let session = cli::Session::new(
        "hmi-media",
        env!("CARGO_PKG_VERSION"),
        &cli::command_name::<Args>(),
        &args.reporting,
    );
    let outcome = run(&args, &session);
    session.finish(outcome)
}
