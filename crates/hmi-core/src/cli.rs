//! 各独立工具共用的 JSON 结果格式和错误输出。

use crate::{
    Result,
    diagnostic::{Diagnostic, Severity},
};
use serde_json::{Value, json};
use std::process::ExitCode;

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

/// 参数错误在 JSON 模式下也输出相同结构；帮助和版本仍使用常规文字。
pub fn parse<T: clap::Parser>() -> std::result::Result<T, ExitCode> {
    match T::try_parse() {
        Ok(args) => Ok(args),
        Err(error) => {
            let code = error.exit_code();
            if code == 0 || !std::env::args_os().any(|arg| arg == "--json") {
                let _ = error.print();
            } else {
                println!(
                    "{}",
                    json!({"schema_version":1,"ok":false,"result":null,"diagnostics":[Diagnostic::error("arguments",error.to_string())]})
                );
            }
            Err(ExitCode::from(code as u8))
        }
    }
}

pub fn finish(outcome: Result<CommandResult>, json_mode: bool) -> ExitCode {
    let (ok, value, message, diagnostics) = match outcome {
        Ok(outcome) => (true, outcome.result, outcome.message, outcome.diagnostics),
        Err(error) => {
            let diagnostic = error
                .as_diagnostic()
                .cloned()
                .unwrap_or_else(|| Diagnostic::error("execution", error.to_string()));
            (false, Value::Null, String::new(), vec![diagnostic])
        }
    };
    if json_mode {
        println!(
            "{}",
            json!({"schema_version":1,"ok":ok,"result":value,"diagnostics":diagnostics})
        );
    } else {
        for diagnostic in &diagnostics {
            let kind = if diagnostic.severity == Severity::Error {
                "错误"
            } else {
                "警告"
            };
            let file = diagnostic.location.file.as_deref().unwrap_or("");
            eprintln!(
                "{kind} [{file}] {}: {}",
                diagnostic.code, diagnostic.message
            );
        }
        if ok {
            println!("{message}");
        }
    }
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
