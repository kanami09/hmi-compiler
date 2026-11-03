//! 脚本辅助诊断。
use crate::diagnostic::{Diagnostic, Location};
use crate::project::Project;
use std::collections::BTreeSet;

pub type ScriptSource = (Option<usize>, Option<usize>, String, String);

#[derive(Debug)]
struct Token {
    text: String,
    line: usize,
}

fn tokenize(text: &str) -> Vec<Token> {
    let bytes = text.as_bytes();
    let mut tokens = Vec::new();
    let (mut i, mut line) = (0, 1);
    while i < bytes.len() {
        match bytes[i] {
            b'\n' => {
                line += 1;
                i += 1;
            }
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i < bytes.len() {
                    if bytes[i] == b'\n' {
                        line += 1;
                    }
                    if bytes[i] == b'*' && bytes.get(i + 1) == Some(&b'/') {
                        i += 2;
                        break;
                    }
                    i += 1;
                }
            }
            quote @ (b'"' | b'\'') => {
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == b'\n' {
                        line += 1;
                    }
                    if bytes[i] == b'\\' {
                        i = (i + 2).min(bytes.len());
                    } else if bytes[i] == quote {
                        i += 1;
                        break;
                    } else {
                        i += 1;
                    }
                }
            }
            b if b.is_ascii_alphanumeric() || b == b'_' => {
                let start = i;
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                tokens.push(Token {
                    text: text[start..i].into(),
                    line,
                });
            }
            b if b.is_ascii_whitespace() => i += 1,
            b if b.is_ascii() => {
                tokens.push(Token {
                    text: (b as char).to_string(),
                    line,
                });
                i += 1;
            }
            _ => i += 1,
        }
    }
    tokens
}

fn bracket(tokens: &[Token], start: usize) -> Option<(&[Token], usize)> {
    if tokens.get(start)?.text != "[" {
        return None;
    }
    let mut depth = 1;
    for (i, token) in tokens.iter().enumerate().skip(start + 1) {
        if token.text == "[" {
            depth += 1;
        }
        if token.text == "]" {
            depth -= 1;
        }
        if depth == 0 {
            return Some((&tokens[start + 1..i], i + 1));
        }
    }
    None
}

fn number(tokens: &[Token]) -> Option<i64> {
    match tokens {
        [token] => token
            .text
            .strip_prefix("0x")
            .or_else(|| token.text.strip_prefix("0X"))
            .map(|hex| i64::from_str_radix(hex, 16).ok())
            .unwrap_or_else(|| token.text.parse().ok()),
        [minus, token] if minus.text == "-" => {
            token.text.parse::<i64>().ok().and_then(i64::checked_neg)
        }
        _ => None,
    }
}

struct Checker<'a> {
    project: &'a Project,
    page: Option<usize>,
    object: Option<usize>,
    file: &'a str,
    diagnostics: &'a mut Vec<Diagnostic>,
    seen: BTreeSet<(String, usize)>,
}

impl Checker<'_> {
    fn warn(&mut self, code: &str, message: String, line: usize) {
        if !self.seen.insert((code.into(), line)) {
            return;
        }
        let page = self
            .page
            .and_then(|i| self.project.pages.get(i))
            .map(|(_, p)| p);
        let object = page.and_then(|p| self.object.and_then(|i| p.objects.get(i)));
        self.diagnostics
            .push(Diagnostic::warning(code, message).at(Location {
                file: Some(self.file.into()),
                page: page.map(|p| p.name.clone()),
                object: object.map(|o| o.name.clone()),
                line: Some(line),
                ..Location::default()
            }));
    }

    fn reference(&mut self, page_id: usize, object_id: usize, field: Option<&str>, line: usize) {
        let Some((_, page)) = self.project.pages.get(page_id) else {
            self.warn("script_page", format!("页面 ID {page_id} 不存在"), line);
            return;
        };
        let Some(object) = page.objects.get(object_id) else {
            self.warn(
                "script_object",
                format!("页面 {} 的控件 ID {object_id} 不存在", page.name),
                line,
            );
            return;
        };
        if let Some(field) = field
            && object.attribute(field).is_none()
        {
            self.warn(
                "script_property",
                format!("{}.{} 没有属性 {field}", page.name, object.name),
                line,
            );
        }
        if self.page != Some(page_id) && object.integer("vscope") != Some(1) {
            self.warn(
                "script_scope",
                format!(
                    "跨页面访问 {}.{}，但控件不是全局作用域",
                    page.name, object.name
                ),
                line,
            );
        }
    }

    fn named(&mut self, page_name: Option<&str>, object_name: &str, field: &str, line: usize) {
        let page_id = if let Some(name) = page_name {
            match self.project.pages.iter().position(|(_, p)| p.name == name) {
                Some(i) => i,
                None => {
                    self.warn("script_page", format!("页面 {name} 不存在"), line);
                    return;
                }
            }
        } else if let Some(i) = self.page {
            i
        } else {
            self.warn(
                "script_context",
                format!("全局代码中的 {object_name}.{field} 依赖当前页面，无法静态确认"),
                line,
            );
            return;
        };
        let page = &self.project.pages[page_id].1;
        if let Some(i) = page.objects.iter().position(|o| o.name == object_name) {
            self.reference(page_id, i, Some(field), line);
        } else {
            self.warn(
                "script_object",
                format!("页面 {} 没有控件 {object_name}", page.name),
                line,
            );
        }
    }

    fn array(&mut self, tokens: &[Token], start: usize, page_id: Option<usize>) -> usize {
        let line = tokens[start].line;
        let Some((expression, end)) = bracket(tokens, start + 1) else {
            return start + 1;
        };
        let object_id = number(expression);
        if let (Some(page_id), Some(object_id)) = (page_id, object_id) {
            if object_id < 0 {
                self.warn("script_object", format!("控件 ID {object_id} 不存在"), line);
            } else {
                let field = tokens
                    .get(end)
                    .filter(|t| t.text == ".")
                    .and_then(|_| tokens.get(end + 1))
                    .map(|t| t.text.as_str());
                self.reference(page_id, object_id as usize, field, line);
            }
        } else {
            self.warn(
                "script_dynamic",
                "名称组含动态 ID 或依赖当前页面，无法静态确认目标".into(),
                line,
            );
        }
        end
    }

    fn scan(&mut self, text: &str) {
        let tokens = tokenize(text);
        let mut i = 0;
        while i < tokens.len() {
            let t = &tokens[i];
            if t.text == "p"
                && let Some((expression, end)) = bracket(&tokens, i + 1)
            {
                let page_id = number(expression);
                let valid_page = page_id
                    .and_then(|n| usize::try_from(n).ok())
                    .filter(|n| *n < self.project.pages.len());
                if let Some(page_id) = page_id
                    && valid_page.is_none()
                {
                    self.warn("script_page", format!("页面 ID {page_id} 不存在"), t.line);
                }
                if page_id.is_none() {
                    self.warn(
                        "script_dynamic",
                        "页面名称组含动态 ID，无法静态确认目标".into(),
                        t.line,
                    );
                }
                if tokens.get(end).is_some_and(|t| t.text == ".")
                    && tokens.get(end + 1).is_some_and(|t| t.text == "b")
                {
                    i = self.array(&tokens, end + 1, valid_page);
                    continue;
                }
                i = end;
                continue;
            }
            if t.text == "b" && tokens.get(i + 1).is_some_and(|t| t.text == "[") {
                i = self.array(&tokens, i, self.page);
                continue;
            }
            if tokens.get(i + 1).is_some_and(|t| t.text == ".")
                && tokens.get(i + 2).is_some()
                && t.text.as_bytes()[0].is_ascii_alphabetic()
            {
                let second = &tokens[i + 2].text;
                if second == "b" && tokens.get(i + 3).is_some_and(|t| t.text == "[") {
                    let page_id = self
                        .project
                        .pages
                        .iter()
                        .position(|(_, p)| p.name == t.text);
                    if page_id.is_none() {
                        self.warn("script_page", format!("页面 {} 不存在", t.text), t.line);
                    }
                    i = self.array(&tokens, i + 2, page_id);
                    continue;
                }
                if tokens.get(i + 3).is_some_and(|t| t.text == ".")
                    && let Some(field) = tokens.get(i + 4)
                {
                    self.named(Some(&t.text), second, &field.text, t.line);
                    i += 5;
                    continue;
                }
                // 方法调用属于控件 API，不将方法名误报为属性。
                if !tokens.get(i + 3).is_some_and(|t| t.text == "(") {
                    self.named(None, &t.text, second, t.line);
                }
                i += 3;
                continue;
            }
            // page 常量名称或数字是可静态确认的跳转；表达式交给官方编译器。
            if t.text == "page"
                && let Some(target) = tokens.get(i + 1)
                && target.line == t.line
            {
                let valid = target
                    .text
                    .parse::<usize>()
                    .map(|n| n < self.project.pages.len())
                    .unwrap_or_else(|_| {
                        self.project
                            .pages
                            .iter()
                            .any(|(_, p)| p.name == target.text)
                    });
                let expression = tokens
                    .get(i + 2)
                    .is_some_and(|next| next.line == target.line && next.text != ";");
                if !valid && !expression {
                    self.warn(
                        "script_page_target",
                        format!("页面跳转目标 {} 无法静态确认", target.text),
                        t.line,
                    );
                }
            }
            i += 1;
        }
    }
}

pub fn check(project: &Project, sources: &[ScriptSource]) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let fonts = project.references.iter().filter(|r| r.kind == "zi").count();
    let images = project.references.iter().filter(|r| r.kind == "i").count();
    for (_, page) in &project.pages {
        for object in &page.objects {
            let mut keys = vec![("font", fonts, 255)];
            let picture_key = match (object.type_code, object.integer("sta")) {
                (112, _) | (121, Some(1)) => Some("pic"),
                (_, Some(2)) => Some("pic"),
                (113, _) | (_, Some(0)) if object.attribute("picc").is_some() => Some("picc"),
                _ => None,
            };
            if let Some(key) = picture_key {
                keys.push((key, images, 65535));
            }
            for (key, count, sentinel) in keys {
                if let Some(id) = object.integer(key)
                    && id != sentinel
                    && id as usize >= count
                {
                    diagnostics.push(
                        Diagnostic::warning(
                            "property_resource",
                            format!(
                                "{}.{}.{} 引用 ID {id}，但对应资源仅有 {count} 个",
                                page.name, object.name, key
                            ),
                        )
                        .at(Location {
                            page: Some(page.name.clone()),
                            object: Some(object.name.clone()),
                            property: Some(key.into()),
                            ..Location::default()
                        }),
                    );
                }
            }
        }
    }
    for (page, object, file, text) in sources {
        Checker {
            project,
            page: *page,
            object: *object,
            file,
            diagnostics: &mut diagnostics,
            seen: BTreeSet::new(),
        }
        .scan(text);
    }
    diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ignores_comments_and_escaped_strings() {
        let tokens = tokenize(
            "// missing.obj.val\n t0.txt=\"missing.\\\"obj.val\" /* x.y.z */\n b[2].val=1",
        );
        assert!(
            !tokens
                .iter()
                .any(|t| t.text == "missing" || t.text == "obj" || t.text == "x")
        );
        assert_eq!(tokens.iter().find(|t| t.text == "b").unwrap().line, 3);
    }
}
