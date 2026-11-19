//! 五个命令行程序共用的输出协议；人类显示与 JSON 使用同一份执行报告。
use crate::{Result, diagnostic::Diagnostic, report::Report};
use clap::{Args, ValueEnum};
use serde_json::{Value, json};
use std::{
    io::{self, IsTerminal, Write},
    process::ExitCode,
    sync::{Arc, Mutex, mpsc},
    thread,
    time::Duration,
};

mod human;

#[derive(Clone, Copy, Debug, ValueEnum, Default)]
pub enum Color {
    #[default]
    Auto,
    Always,
    Never,
}
#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum MessageFormat {
    Human,
    Json,
    Jsonl,
}
#[derive(Clone, Debug, Args, Default)]
pub struct OutputOptions {
    #[arg(
        long,
        global = true,
        conflicts_with = "message_format",
        help = "输出单个 JSON 结果"
    )]
    pub json: bool,
    #[arg(
        long,
        global = true,
        value_enum,
        help = "输出格式：human、json 或 jsonl"
    )]
    pub message_format: Option<MessageFormat>,
    #[arg(
        short = 'v',
        long,
        global = true,
        conflicts_with = "quiet",
        help = "显示任务明细"
    )]
    pub verbose: bool,
    #[arg(short = 'q', long, global = true, help = "仅显示诊断和查询结果")]
    pub quiet: bool,
    #[arg(
        long,
        global = true,
        value_enum,
        default_value = "auto",
        help = "颜色输出策略"
    )]
    pub color: Color,
}
impl OutputOptions {
    pub fn format(&self) -> MessageFormat {
        if self.json {
            MessageFormat::Json
        } else {
            self.message_format.unwrap_or(MessageFormat::Human)
        }
    }
    fn raw() -> Self {
        let args = std::env::args()
            .skip(1)
            .take_while(|a| a != "--")
            .collect::<Vec<_>>();
        let mut options = Self {
            json: args.iter().any(|a| a == "--json"),
            verbose: args.iter().any(|a| a == "-v" || a == "--verbose"),
            quiet: args.iter().any(|a| a == "-q" || a == "--quiet"),
            ..Self::default()
        };
        for (index, arg) in args.iter().enumerate() {
            let value = |name: &str| {
                arg.strip_prefix(&format!("{name}="))
                    .map(String::from)
                    .or_else(|| {
                        if arg == name {
                            args.get(index + 1).cloned()
                        } else {
                            None
                        }
                    })
            };
            if let Some(value) = value("--message-format") {
                options.message_format = match value.as_str() {
                    "json" => Some(MessageFormat::Json),
                    "jsonl" => Some(MessageFormat::Jsonl),
                    _ => None,
                };
            }
            if let Some(value) = value("--color") {
                options.color = match value.as_str() {
                    "always" => Color::Always,
                    "never" => Color::Never,
                    _ => Color::Auto,
                };
            }
        }
        options
    }
}
pub struct CommandResult {
    pub result: Value,
    pub message: String,
    pub diagnostics: Vec<Diagnostic>,
}
pub fn success(result: Value, message: impl Into<String>) -> CommandResult {
    CommandResult {
        result,
        message: message.into(),
        diagnostics: vec![],
    }
}

pub fn command_name<T: clap::Parser>() -> String {
    let mut command = T::command();
    let mut names = vec![];
    for arg in std::env::args().skip(1) {
        if let Some(child) = command.find_subcommand(&arg).cloned() {
            names.push(child.get_name().to_owned());
            command = child;
        }
    }
    if names.is_empty() {
        T::command()
            .get_name()
            .trim_start_matches("hmi-")
            .to_owned()
    } else {
        names.join(".")
    }
}

/// 参数错误、帮助与版本同样经过协议层；stdout 不出现混合格式。
pub fn parse<T: clap::Parser>() -> std::result::Result<T, ExitCode> {
    match T::try_parse() {
        Ok(args) => Ok(args),
        Err(error) => {
            let code = error.exit_code();
            let options = OutputOptions::raw();
            if matches!(options.format(), MessageFormat::Human) {
                let _ = error.print();
            } else {
                use clap::error::ErrorKind;
                let command = T::command();
                let name = command.get_name();
                let info = match error.kind() {
                    ErrorKind::DisplayVersion => "version",
                    ErrorKind::DisplayHelp
                    | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
                        if code == 0 =>
                    {
                        "help"
                    }
                    _ => "arguments",
                };
                let mut diagnostics = if code == 0 {
                    vec![]
                } else {
                    vec![Diagnostic::error("arguments", error.to_string())]
                };
                for diagnostic in &mut diagnostics {
                    diagnostic.stage = Some("arguments".into());
                }
                let value = json!({"schema_version":2,"tool":{"name":name,"version":command.get_version().unwrap_or("")},
                    "command":if code==0 {info.to_owned()}else{command_name::<T>()},"ok":code==0,"project":null,
                    "result":if code==0 {json!({if info=="version" {"version"} else {"help_text"}:if info=="version" {command.get_version().unwrap_or("").to_owned()}else{error.to_string()}})}else{Value::Null},
                    "diagnostics":diagnostics});
                if matches!(options.format(), MessageFormat::Jsonl) {
                    println!(
                        "{}",
                        json!({"schema_version":2,"event":"result","data":value})
                    );
                } else {
                    println!("{value}");
                }
            }
            Err(ExitCode::from(code as u8))
        }
    }
}

struct Sink {
    options: OutputOptions,
    lock: Mutex<EventOrder>,
    color: bool,
}
#[derive(Default)]
struct EventOrder {
    started: std::collections::BTreeSet<u64>,
    finished: std::collections::BTreeSet<u64>,
}
impl EventOrder {
    fn accept(&mut self, event: &Value) -> bool {
        let kind = event["event"].as_str().unwrap_or("");
        let id = event["task"]["id"]
            .as_u64()
            .or_else(|| event["task_id"].as_u64());
        if let Some(id) = id {
            match kind {
                "task_started" => {
                    self.started.insert(id);
                }
                "task_finished" => {
                    self.finished.insert(id);
                }
                // 心跳拿到快照后任务可能已经结束，不能再输出倒退的进度。
                "task_progress" if !self.started.contains(&id) || self.finished.contains(&id) => {
                    return false;
                }
                _ => {}
            }
        }
        true
    }
}
impl Sink {
    fn event(&self, mut event: Value) {
        let mut order = self.lock.lock().unwrap();
        if !order.accept(&event) {
            return;
        }
        match self.options.format() {
            MessageFormat::Json => {}
            MessageFormat::Jsonl => {
                let name = event.as_object_mut().unwrap().remove("event").unwrap();
                println!("{}", json!({"schema_version":2,"event":name,"data":event}));
            }
            MessageFormat::Human => human::event(&event, &self.options, self.color),
        }
    }
}
pub struct Session {
    pub report: Report,
    tool: String,
    version: String,
    command: String,
    sink: Arc<Sink>,
    stop: Option<mpsc::Sender<()>>,
    heartbeat: Option<thread::JoinHandle<()>>,
}
impl Session {
    pub fn new(tool: &str, version: &str, command: &str, options: &OutputOptions) -> Self {
        let color = match options.color {
            Color::Always => true,
            Color::Never => false,
            Color::Auto => io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none(),
        };
        let sink = Arc::new(Sink {
            options: options.clone(),
            lock: Mutex::new(EventOrder::default()),
            color,
        });
        let listener = Arc::clone(&sink);
        let report = Report::new(Some(Arc::new(move |event| listener.event(event))));
        let (stop, heartbeat) = if matches!(options.format(), MessageFormat::Json) {
            (None, None)
        } else {
            let (sender, receiver) = mpsc::channel();
            let progress = report.clone();
            (
                Some(sender),
                Some(thread::spawn(move || {
                    while receiver.recv_timeout(Duration::from_secs(5))
                        == Err(mpsc::RecvTimeoutError::Timeout)
                    {
                        progress.progress();
                    }
                })),
            )
        };
        let session = Self {
            report,
            tool: tool.into(),
            version: version.into(),
            command: command.into(),
            sink,
            stop,
            heartbeat,
        };
        session
            .sink
            .event(json!({"event":"command_started","tool":tool,"command":command}));
        session
    }
    fn stop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(handle) = self.heartbeat.take() {
            let _ = handle.join();
        }
    }
    pub fn finish(mut self, outcome: Result<CommandResult>) -> ExitCode {
        self.stop();
        let (ok, value, message, mut diagnostics) = match outcome {
            Ok(outcome) => (true, outcome.result, outcome.message, outcome.diagnostics),
            Err(error) => (false, Value::Null, String::new(), error.diagnostics()),
        };
        for diagnostic in &mut diagnostics {
            self.report.normalize(diagnostic);
        }
        // 源位置可比较的诊断保持稳定顺序，避免并行完成顺序影响 Agent 的处理。
        diagnostics.sort_by(|a, b| {
            (
                &a.location.file,
                &a.location.line,
                &a.location.column,
                &a.code,
            )
                .cmp(&(
                    &b.location.file,
                    &b.location.line,
                    &b.location.column,
                    &b.code,
                ))
        });
        let value = self.report.result(value);
        let result = json!({"schema_version":2,"tool":{"name":self.tool,"version":self.version},"command":self.command,
            "ok":ok,"project":self.report.project_value(),"result":value,"diagnostics":diagnostics});
        let _guard = self.sink.lock.lock().unwrap();
        match self.sink.options.format() {
            MessageFormat::Json => println!("{result}"),
            MessageFormat::Jsonl => {
                for diagnostic in &diagnostics {
                    println!(
                        "{}",
                        json!({"schema_version":2,"event":"diagnostic","data":diagnostic})
                    );
                }
                println!(
                    "{}",
                    json!({"schema_version":2,"event":"result","data":result})
                );
            }
            MessageFormat::Human => {
                for diagnostic in &diagnostics {
                    human::diagnostic(diagnostic, self.sink.color);
                }
                if ok {
                    if let Some(query) = self.report.query_text().or_else(|| {
                        matches!(
                            self.command.as_str(),
                            "inspect" | "probe" | "backend" | "target.list" | "target.show"
                        )
                        .then(|| human::query(&self.command, &value))
                    }) {
                        println!("{query}");
                    } else if !self.sink.options.quiet {
                        human::completed(&message, &value, &self.command, self.sink.color);
                    }
                } else {
                    let operation = match self.command.as_str() {
                        "build" | "rebuild" => "构建未完成",
                        "page" | "program" => "编译未完成",
                        "encode" | "builtin" => "媒体处理未完成",
                        "import" => "导入未完成",
                        "export" | "plan" | "target.export" => "导出未完成",
                        "clean" => "清理未完成",
                        _ => "命令未完成",
                    };
                    human::line(
                        "失败",
                        &format!(
                            "{operation}（用时 {:.2} 秒）",
                            self.report.elapsed_ms() as f64 / 1000.0
                        ),
                        self.sink.color,
                    );
                    if let Some(outputs) = value["outputs"].as_array() {
                        for output in outputs {
                            let label = if output["state"] == "written" {
                                "已写入"
                            } else if output["kind"] == "hmi" {
                                "HMI 输出未更新"
                            } else {
                                "未完成写入"
                            };
                            eprintln!(
                                "         {label}：{}",
                                human::display_path(output["path"].as_str().unwrap_or(""))
                            );
                        }
                    }
                }
            }
        }
        let _ = io::stdout().flush();
        let _ = io::stderr().flush();
        if ok {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        }
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        self.stop();
    }
}
