//! 人类日志只负责展示，不根据文件存在与否猜测执行结果。
use super::OutputOptions;
use crate::diagnostic::{Diagnostic, Severity};
use serde_json::Value;
use std::{fs, path::Path};

fn plain(text: &str) -> String {
    text.replace(['\r', '\n', '\u{1b}'], " ")
}
pub fn line(action: &str, subject: &str, color: bool) {
    let subject = match action {
        "preserve_native" => format!("{subject}（原生资产包）"),
        "package_encoded" => format!("{subject}（已编码输入）"),
        "reuse_original" => format!("{subject}（原始编码）"),
        _ => subject.to_owned(),
    };
    let action = match action {
        "compile" => "编译",
        "encode" => "编码",
        "process" => "处理",
        "package_encoded" => "封装",
        "preserve_native" => "保留",
        "reuse_original" => "复用",
        "check" => "检查",
        "cache_check" => "检查缓存",
        "cache_hit" => "复用",
        "link" => "链接",
        "validate" => "校验",
        "publish" => "写入",
        "import" => "导入",
        "parse" => "解析",
        "export" => "导出",
        "init" => "创建",
        "clean" => "清理",
        "build" => "构建",
        other => other,
    };
    let subject = if Path::new(&subject).is_absolute() {
        display_path(&subject)
    } else {
        subject
    };
    let subject = plain(&subject);
    if color {
        let code = match action {
            "错误" | "失败" => "31",
            "警告" => "33",
            _ => "32",
        };
        eprintln!("    \x1b[1;{code}m{action}\x1b[0m {subject}");
    } else {
        eprintln!("    {action} {subject}");
    }
}
pub fn event(event: &Value, options: &OutputOptions, color: bool) {
    if options.quiet {
        return;
    }
    let text = |v: &Value, key: &str| v[key].as_str().unwrap_or("").to_owned();
    match event["event"].as_str().unwrap_or("") {
        "stage_started" => line(&text(event, "action"), &text(event, "subject"), color),
        "task_started" => {
            let task = &event["task"];
            let action = text(task, "action");
            if !matches!(action.as_str(), "cache_hit" | "cache_check") || options.verbose {
                line(
                    if action == "cache_hit" {
                        "复用"
                    } else {
                        &action
                    },
                    &text(task, "subject"),
                    color,
                );
            }
        }
        "task_finished" if options.verbose => {
            let task = &event["task"];
            if task["state"] != "failed" {
                eprintln!(
                    "         {}：{}，用时 {} 毫秒",
                    plain(&text(task, "subject")),
                    if task["state"] == "reused" {
                        "构建缓存"
                    } else {
                        "完成"
                    },
                    task["elapsed_ms"]
                );
            }
        }
        "task_action"
            if matches!(
                event["task"]["action"].as_str(),
                Some(
                    "reuse_original" | "preserve_native" | "package_encoded" | "encode" | "process"
                )
            ) =>
        {
            line(
                &text(&event["task"], "action"),
                &text(&event["task"], "subject"),
                color,
            );
        }
        "task_action" if options.verbose && event["task"]["action"] == "cache_hit" => line(
            "复用",
            &format!("{}（构建缓存）", text(&event["task"], "subject")),
            color,
        ),
        "task_progress" => {
            let metric = &event["progress"];
            let detail = if let Some(current) = metric["current"].as_u64() {
                let phase = match metric["phase"].as_str() {
                    Some("decode") => "已解码",
                    Some("encode") => "已编码",
                    Some("resample") => "已转换",
                    _ => "已处理",
                };
                let unit = match metric["unit"].as_str() {
                    Some("frames") => "帧",
                    Some("sample_frames") => "个采样帧",
                    _ => "项",
                };
                let count = metric["total"]
                    .as_u64()
                    .map(|total| format!("{current}/{total}"))
                    .unwrap_or_else(|| current.to_string());
                format!("{phase} {count} {unit}")
            } else {
                "正在处理".into()
            };
            line(
                "进度",
                &format!(
                    "{}：{detail}，用时 {:.2} 秒",
                    text(event, "subject"),
                    event["elapsed_ms"].as_u64().unwrap_or(0) as f64 / 1000.0
                ),
                color,
            );
        }
        "detail" if options.verbose => eprintln!("         {}", plain(&text(event, "message"))),
        "output_written" if options.verbose => line("写入", &text(&event["output"], "path"), color),
        _ => {}
    }
}
pub(super) fn display_path(path: &str) -> String {
    let path = Path::new(path);
    let current = std::env::current_dir()
        .ok()
        .and_then(|p| fs::canonicalize(p).ok());
    let path = if let Some(base) = &current {
        path.strip_prefix(base).unwrap_or(path)
    } else {
        path
    };
    path.display().to_string()
}
pub fn diagnostic(d: &Diagnostic, color: bool) {
    let action = if d.severity == Severity::Error {
        "错误"
    } else {
        "警告"
    };
    line(action, &format!("[{}]：{}", d.code, d.message), color);
    let loc = &d.location;
    if let Some(file) = &loc.file {
        let mut position = display_path(file);
        if let Some(line) = loc.line {
            position.push_str(&format!(":{line}"));
        }
        if let Some(column) = loc.column {
            position.push_str(&format!(":{column}"));
        }
        eprintln!("  --> {}", plain(&position));
        if let Some(number) = loc.line.and_then(|n| n.checked_sub(1))
            && let Ok(source) = fs::read_to_string(file)
            && let Some(source) = source.lines().nth(number)
        {
            eprintln!("   |\n{:>3} | {}", number + 1, source.replace('\t', "    "));
            if let Some(column) = loc
                .column
                .filter(|c| *c > 0 && *c <= source.chars().count() + 1)
            {
                let prefix = source
                    .chars()
                    .take(column - 1)
                    .map(|c| {
                        if c == '\t' {
                            4
                        } else {
                            unicode_width::UnicodeWidthChar::width(c).unwrap_or(0)
                        }
                    })
                    .sum::<usize>();
                let width = loc
                    .end_column
                    .and_then(|end| end.checked_sub(column))
                    .filter(|n| *n > 0)
                    .unwrap_or(1);
                eprintln!("   | {}{}", " ".repeat(prefix), "^".repeat(width.min(120)));
            }
        }
    } else if let Some(source) = d.details.get("source_name").and_then(Value::as_str) {
        eprintln!("   = 源文件：{}", plain(source));
    }
    let mut context = vec![];
    for (name, value) in [
        ("页面", &loc.page),
        ("控件", &loc.object),
        ("属性", &loc.property),
        ("事件", &loc.event),
    ] {
        if let Some(value) = value {
            context.push(format!("{name} {}", plain(value)));
        }
    }
    if !context.is_empty() {
        eprintln!("   = 位置：{}", context.join("，"));
    }
    if d.original_severity == Some(Severity::Warning) && d.severity == Severity::Error {
        eprintln!("   = 此警告因警告策略按错误处理。");
    }
}
pub fn completed(message: &str, result: &Value, command: &str, color: bool) {
    let mut messages = message.lines();
    let message = messages.next().unwrap_or("");
    let message = [
        "重建完成：",
        "编译完成：",
        "空白工程已创建：",
        "新版工程已导入：",
        "新版工程已导出：",
        "构建计划已导出：",
        "目标配置已导出：",
    ]
    .iter()
    .find_map(|prefix| message.strip_prefix(prefix))
    .unwrap_or(message);
    let message = if Path::new(message).is_absolute() {
        display_path(message)
    } else {
        message.to_owned()
    };
    let size = result["size"]
        .as_u64()
        .map(|bytes| {
            if bytes >= 1024 * 1024 {
                format!("{:.2} MiB，", bytes as f64 / (1024.0 * 1024.0))
            } else {
                format!("{:.2} KiB，", bytes as f64 / 1024.0)
            }
        })
        .unwrap_or_default();
    let checking = matches!(command, "check" | "plan");
    line(
        if command == "check" {
            "通过"
        } else {
            "完成"
        },
        &format!(
            "{}（{size}用时 {:.2} 秒）",
            message.trim_end_matches('。'),
            result["elapsed_ms"].as_u64().unwrap_or(0) as f64 / 1000.0
        ),
        color,
    );
    if result["summary"].as_object().is_none_or(|s| s.is_empty()) {
        for message in messages {
            eprintln!("         {message}");
        }
    }
    for (group, label, unit) in [
        ("program", "全局代码", "项"),
        ("pages", "页面", "页"),
        ("media", "媒体", "项"),
    ] {
        if let Some(count) = result["summary"].get(group) {
            if count["total"] == 0 {
                continue;
            }
            let verb = if checking {
                "检查"
            } else if group == "media" {
                "处理"
            } else {
                "编译"
            };
            let mut parts = vec![];
            if count["processed"].as_u64().unwrap_or(0) > 0 {
                parts.push(format!("{verb} {} {unit}", count["processed"]));
            }
            if count["reused"].as_u64().unwrap_or(0) > 0 {
                parts.push(format!("复用 {} {unit}", count["reused"]));
            }
            eprintln!("         {label}：{}", parts.join("，"));
        }
    }
}

pub fn query(command: &str, value: &Value) -> String {
    if command == "target.list" {
        return value["profiles"]
            .as_array()
            .map(|p| {
                p.iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default();
    }
    let target = if value.get("target").is_some() {
        &value["target"]
    } else {
        value
    };
    if let Some(profile) = target["profile"].as_str() {
        let mut lines = vec![
            format!("型号      {profile}"),
            format!("格式      {}", target["format"].as_str().unwrap_or("")),
            format!("编码      {}", target["encoding"].as_str().unwrap_or("")),
            format!("方向      {}°", target["orientation"]),
        ];
        if let Some(canvas) = value
            .get("canvas")
            .filter(|c| c.is_object())
            .or_else(|| target.get("canvas"))
        {
            lines.push(format!(
                "尺寸      {}×{}",
                canvas["width"], canvas["height"]
            ));
        }
        if let Some(pages) = value["pages"].as_array() {
            let ordinary = |page: &Value| {
                page["objects"]
                    .as_array()
                    .map_or(0, |o| o.len().saturating_sub(1))
            };
            lines.extend([
                format!("页面      {}", pages.len()),
                format!("控件      {}", pages.iter().map(ordinary).sum::<usize>()),
                "结构校验  通过".into(),
                String::new(),
                "ID  页面名称  控件数量".into(),
            ]);
            lines.extend(pages.iter().map(|p| {
                format!(
                    "{}   {}   {}",
                    p["id"],
                    p["name"].as_str().unwrap_or(""),
                    ordinary(p)
                )
            }));
            if let Some(resources) = value["active_resources"].as_array() {
                lines.push(format!("\n活动资源  {}", resources.len()));
                lines.extend(
                    resources
                        .iter()
                        .map(|r| r["name"].as_str().unwrap_or("").to_owned()),
                );
            }
        } else if let Some(controls) = target["capabilities"]["control_types"].as_array() {
            lines.push(format!(
                "控件      {}",
                controls
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join("、")
            ));
        }
        return lines.join("\n");
    }
    let mut data = value.clone();
    if let Some(data) = data.as_object_mut() {
        data.remove("elapsed_ms");
        data.remove("summary");
        data.remove("outputs");
    }
    serde_json::to_string_pretty(&data).unwrap_or_default()
}
