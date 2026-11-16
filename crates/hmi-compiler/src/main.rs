use clap::{Parser, Subcommand};
use hmi_artifact::CompiledObject;
use hmi_core::{
    Result,
    cli::{self, CommandResult},
    file,
    target::Target,
};
use std::{path::PathBuf, process::ExitCode};

#[derive(Parser)]
#[command(name = "hmi-compile", version, about = "编译 HMI 页面和全局代码")]
struct Args {
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    /// 编译页面、控件及事件，支持范围见目标配置。
    Page {
        source: PathBuf,
        #[arg(long)]
        target: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// 编译全局代码。
    Program {
        source: PathBuf,
        #[arg(long)]
        target: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// 查看目标文件描述及载荷摘要。
    Inspect { file: PathBuf },
}
fn run(args: &Args) -> Result<CommandResult> {
    match &args.command {
        Command::Inspect { file } => {
            let object = CompiledObject::read(file)?;
            let mut records = Vec::new();
            if let Some(layout) = &object.metadata.layout {
                for (index, record) in layout.objects.iter().enumerate() {
                    let parsed = hmi_format::page::Object::parse(object.record(index)?)?;
                    let mut properties = if index == 0 {
                        serde_json::json!({"w":parsed.integer("w"),"h":parsed.integer("h"),"sta":parsed.integer("sta"),"bco":parsed.integer("bco")})
                    } else {
                        let definition = hmi_core::controls::by_code(parsed.type_code)?;
                        let attributes = parsed
                            .attributes
                            .iter()
                            .map(|a| (a.name.clone(), a.value.clone()))
                            .collect::<Vec<_>>();
                        definition.validate_record(&parsed.name, &attributes)?;
                        serde_json::json!(definition.properties_from_record(&attributes)?)
                    };
                    for relocation in &object.metadata.relocations {
                        if relocation.object == index
                            && matches!(
                                relocation.kind,
                                hmi_artifact::RelocationKind::ImageId
                                    | hmi_artifact::RelocationKind::FontId
                                    | hmi_artifact::RelocationKind::ExternalPath
                                    | hmi_artifact::RelocationKind::AnimationId
                                    | hmi_artifact::RelocationKind::VideoId
                            )
                        {
                            properties[&relocation.property] =
                                serde_json::json!(relocation.reference);
                        }
                    }
                    records.push(serde_json::json!({"record":index,"name":record.name,"type":hmi_format::page::type_name(record.type_code),"properties":properties,"id_pending":index>0}));
                }
            }
            Ok(cli::success(
                serde_json::json!({"metadata":object.metadata,"objects":records,"payload_size":object.payload.len(),"payload_sha256":hmi_core::sha256(&object.payload)}),
                "目标文件校验通过。",
            ))
        }
        Command::Page {
            source,
            target,
            output,
        }
        | Command::Program {
            source,
            target,
            output,
        } => {
            let config: Target = file::read_json(target)?;
            let compilation = if matches!(args.command, Command::Page { .. }) {
                hmi_page_compiler::compile_page(source, &config)?
            } else {
                hmi_page_compiler::compile_program(source, &config)?
            };
            let mut inputs: Vec<_> = compilation.inputs.iter().map(|p| p.as_path()).collect();
            inputs.push(target);
            compilation.object.write(output, &inputs)?;
            Ok(cli::success(
                serde_json::json!({"output":output,"kind":compilation.object.metadata.kind}),
                format!("编译完成：{}", output.display()),
            ))
        }
    }
}
fn main() -> ExitCode {
    let args: Args = match cli::parse() {
        Ok(args) => args,
        Err(code) => return code,
    };
    cli::finish(run(&args), args.json)
}
