//! 仅 CLI 使用的终端渲染器；不向构建库注入终端样式。
use anstream::{AutoStream, ColorChoice};
use anstyle::{AnsiColor, Color, Style};
use clap::ValueEnum;
use hmi_compiler::{
    build::{BuildEvent, BuildMode},
    diagnostic::{Diagnostic, Severity},
};
use std::io::{IsTerminal, Stderr, Write};

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
pub enum ColorMode {
    #[default]
    Auto,
    Always,
    Never,
}

pub struct Renderer {
    stream: AutoStream<Stderr>,
}

fn single_line(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_control() {
                c.escape_default().to_string()
            } else {
                c.to_string()
            }
        })
        .collect()
}

impl Renderer {
    pub fn new(mode: ColorMode) -> Self {
        let stream = std::io::stderr();
        let choice = match mode {
            ColorMode::Always => ColorChoice::Always,
            ColorMode::Never => ColorChoice::Never,
            ColorMode::Auto
                if !stream.is_terminal()
                    || std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty()) =>
            {
                ColorChoice::Never
            }
            ColorMode::Auto => ColorChoice::Auto,
        };
        Self {
            stream: AutoStream::new(stream, choice),
        }
    }
    fn label(status: &str, color: AnsiColor) -> String {
        let style = Style::new().fg_color(Some(Color::Ansi(color))).bold();
        format!("{}[ {status:<7} ]{}", style.render(), style.render_reset())
    }
    pub fn diagnostic(&mut self, d: &Diagnostic, percent: Option<usize>) {
        let (label, color) = if d.severity == Severity::Error {
            ("ERROR", AnsiColor::Red)
        } else {
            ("WARNING", AnsiColor::Yellow)
        };
        let prefix = percent.map(|n| format!("[{n:>3}%] ")).unwrap_or_default();
        let location = &d.location;
        let mut subject = location.page.clone().unwrap_or_else(|| "工程".into());
        if let Some(object) = &location.object
            && Some(object) != location.page.as_ref()
        {
            subject.push('.');
            subject.push_str(object);
        }
        if let Some(event) = &location.event {
            subject.push_str(".events.");
            subject.push_str(event);
        } else if let Some(property) = &location.property {
            subject.push('.');
            subject.push_str(property);
        }
        let at = location
            .file
            .as_ref()
            .map(|file| {
                let file = single_line(file);
                if let Some(line) = location.line {
                    format!(" [{file}:{line}]")
                } else {
                    format!(" [{file}]")
                }
            })
            .unwrap_or_default();
        let option = d
            .category
            .as_ref()
            .map(|name| {
                if d.original_severity == Some(Severity::Warning) {
                    format!(" [-Werror={name}]")
                } else {
                    format!(" [-W{name}]")
                }
            })
            .unwrap_or_default();
        let _ = writeln!(
            self.stream,
            "{prefix}{} {}:{at} {}{option}",
            Self::label(label, color),
            single_line(&subject),
            single_line(&d.message)
        );
        let _ = self.stream.flush();
    }
    pub fn event(&mut self, event: &BuildEvent) {
        match event {
            BuildEvent::Info(message) => {
                let _ = writeln!(
                    self.stream,
                    "{} {}",
                    Self::label("INFO", AnsiColor::Cyan),
                    single_line(message)
                );
            }
            BuildEvent::Diagnostic(d) => self.diagnostic(d, None),
            BuildEvent::Page {
                completed,
                total,
                name,
                diagnostics,
            } => {
                let percent = completed
                    .saturating_mul(100)
                    .checked_div(*total)
                    .unwrap_or(100);
                if diagnostics.is_empty() {
                    let _ = writeln!(
                        self.stream,
                        "[{percent:>3}%] {} {}",
                        Self::label("SUCCESS", AnsiColor::Green),
                        single_line(name)
                    );
                } else {
                    for d in diagnostics {
                        self.diagnostic(d, Some(percent));
                    }
                }
            }
            BuildEvent::Finished(report) => {
                let failed = report.errors != 0;
                let (status, color) = if failed {
                    ("ERROR", AnsiColor::Red)
                } else {
                    ("SUCCESS", AnsiColor::Green)
                };
                let action = if report.mode == BuildMode::Check {
                    "检查"
                } else {
                    "构建"
                };
                let result = if failed { "失败" } else { "完成" };
                let target = if report.output_written {
                    report
                        .output
                        .as_ref()
                        .map(|p| format!("；输出：{}", single_line(p)))
                        .unwrap_or_default()
                } else {
                    String::new()
                };
                let _ = writeln!(
                    self.stream,
                    "{} {action}{result}：{} 个错误，{} 个警告；{} 页重建，{} 页复用；耗时 {:.3}s{target}",
                    Self::label(status, color),
                    report.errors,
                    report.warnings,
                    report.rebuilt_pages,
                    report.reused_pages,
                    report.elapsed_ms as f64 / 1000.0
                );
            }
        }
        let _ = self.stream.flush();
    }
}
