//! 外部部署路径采用设备路径语法，不将任意设备字符串直接拼成本机路径。
use crate::{Result, error};

/// 首版部署到输出目录内的 sd0/；拒绝 Windows 别名、设备名和路径穿越。
pub fn validate(path: &str) -> Result<()> {
    let invalid = || {
        error(
            "deployment_path",
            "外部输出须使用 sd0/ 下的相对文件路径，不能包含空段、.、..、反斜杠或非法文件名",
        )
    };
    if !path.starts_with("sd0/") || path.len() > 255 {
        return Err(invalid());
    }
    let parts = path[4..].split('/').collect::<Vec<_>>();
    if parts.is_empty() {
        return Err(invalid());
    }
    for part in parts {
        let stem = part
            .split('.')
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase();
        let reserved = matches!(
            stem.as_str(),
            "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
        ) || stem
            .strip_prefix("COM")
            .or_else(|| stem.strip_prefix("LPT"))
            .is_some_and(|n| n.len() == 1 && matches!(n.as_bytes()[0], b'1'..=b'9'));
        if part.is_empty()
            || matches!(part, "." | "..")
            || part.ends_with([' ', '.'])
            || part.starts_with(' ')
            || part
                .chars()
                .any(|c| c.is_control() || "\\:<>\"|?*".contains(c))
            || reserved
        {
            return Err(invalid());
        }
    }
    Ok(())
}

pub fn key(path: &str) -> String {
    if path
        .get(..4)
        .is_some_and(|p| p.eq_ignore_ascii_case("sd0/"))
    {
        path.to_ascii_lowercase()
    } else {
        path.into()
    }
}
