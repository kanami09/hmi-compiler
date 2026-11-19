"""将公开 Markdown 转为发布包中的离线 HTML；需要 Python 3.11+ 和 Pandoc。"""

import argparse
import hashlib
from html.parser import HTMLParser
import json
import os
from pathlib import Path
import shutil
import subprocess
import tomllib
from urllib.parse import quote, unquote, urlsplit, urlunsplit
from urllib.request import urlopen


ROOT = Path(__file__).resolve().parents[2]
SUPPORT = Path(__file__).resolve().parent
CSS_VERSION = "5.9.0"
# 固定版本和摘要，构建时不读取上游 main 或 latest。
CSS_FILES = {
    "github-markdown.css": (
        "github-markdown.css",
        "6112686f954db5d3806fb96116d2ab20ad3018469ab1015c587fd8efe7d25cf4",
    ),
    "github-markdown-css-LICENSE.txt": (
        "license",
        "5c932d88256b4ab958f64a856fa48e8bd1f55bc1d96b8149c65689e0c61789d3",
    ),
}


def run_pandoc(executable, *args, content=None):
    return subprocess.run(
        [executable, *map(str, args)], input=content, text=True,
        encoding="utf-8", stdout=subprocess.PIPE, check=True,
    ).stdout


def public_documents():
    """只选主 README、workspace 成员说明、参考页和公开数据说明。"""
    workspace = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
    files = {ROOT / "README.md"}
    for member in workspace["workspace"]["members"]:
        directory = ROOT / member
        files.add(directory / "README.md")
        files.update((directory / "reference").rglob("*.md"))
    files.update((ROOT / "data").glob("*/README.md"))
    return sorted(path.resolve() for path in files)


def destination(source):
    relative = source.relative_to(ROOT)
    if relative == Path("README.md"):
        return Path("README.html")
    if relative == Path("LICENSE"):
        return relative
    if source.suffix == ".md":
        relative = relative.with_suffix(".html")
    return Path("manual") / relative


def relative_url(target, page):
    return quote(Path(os.path.relpath(target, page.parent)).as_posix(), safe="/.-_")


def nodes(value):
    """遍历 Pandoc AST；只处理真实链接，不改代码块或示例里的 .md。"""
    if isinstance(value, dict):
        yield value
        for child in value.values():
            yield from nodes(child)
    elif isinstance(value, list):
        for child in value:
            yield from nodes(child)


def rewrite_links(document, source, documents, attachments, tracked):
    for node in nodes(document):
        if node.get("t") not in {"Link", "Image"}:
            continue
        target = node["c"][-1]
        url = urlsplit(target[0])
        if url.scheme or url.netloc or not url.path:
            continue
        referenced = (source.parent / unquote(url.path)).resolve()
        # 包含私有目录或未登记的本地附件时直接失败，不默默打入发布包。
        if referenced not in tracked or not referenced.is_file():
            raise ValueError(f"{source.relative_to(ROOT)} 引用了未跟踪或缺失文件：{url.path}")
        if referenced.suffix == ".md":
            if referenced not in documents:
                raise ValueError(f"未纳入公开文档的 Markdown：{url.path}")
        else:
            attachments.add(referenced)
        path = relative_url(destination(referenced), destination(source))
        target[0] = urlunsplit(("", "", path, url.query, url.fragment))


def copy_css(cache, output):
    cache.mkdir(parents=True, exist_ok=True)
    for name, (upstream, expected) in CSS_FILES.items():
        cached = cache / name
        if not cached.exists():
            url = (
                "https://raw.githubusercontent.com/sindresorhus/"
                f"github-markdown-css/v{CSS_VERSION}/{upstream}"
            )
            with urlopen(url, timeout=60) as response:
                content = response.read()
        else:
            content = cached.read_bytes()
        if hashlib.sha256(content).hexdigest() != expected:
            raise ValueError(f"样式依赖校验失败：{name}")
        cached.write_bytes(content)
        (output / name).write_bytes(content)
    shutil.copyfile(SUPPORT / "layout.css", output / "layout.css")


class HtmlLinks(HTMLParser):
    def __init__(self):
        super().__init__()
        self.ids = set()
        self.links = []

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if "id" in attrs:
            self.ids.add(attrs["id"])
        for attribute in ("href", "src"):
            if attribute in attrs:
                self.links.append(attrs[attribute])


def validate_html(output):
    """用生成后的 HTML 检查文件和锚点，捕捉 Pandoc 标题 ID 差异。"""
    pages = {}
    count = 0
    for page in output.rglob("*.html"):
        parser = HtmlLinks()
        parser.feed(page.read_text(encoding="utf-8"))
        pages[page.resolve()] = parser
    for page, parser in pages.items():
        for link in parser.links:
            url = urlsplit(link)
            if url.scheme or url.netloc:
                continue
            target = (page.parent / unquote(url.path)).resolve() if url.path else page
            if not target.is_relative_to(output) or not target.is_file():
                raise ValueError(f"HTML 本地链接无效：{page.relative_to(output)} -> {link}")
            if url.fragment and target in pages:
                if unquote(url.fragment) not in pages[target].ids:
                    raise ValueError(f"HTML 锚点无效：{page.relative_to(output)} -> {link}")
            count += 1
    return len(pages), count


def copy_attachments(attachments, tracked, output):
    # Schema 使用相邻 $ref；将依赖一起复制，保证离线阅读和校验可用。
    copied = set()
    while attachments - copied:
        source = min(attachments - copied)
        target = output / destination(source)
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, target)
        copied.add(source)
        if source.suffix == ".json":
            for node in nodes(json.loads(source.read_text(encoding="utf-8"))):
                ref = urlsplit(node.get("$ref", ""))
                if not ref.scheme and not ref.netloc and ref.path:
                    dependency = (source.parent / unquote(ref.path)).resolve()
                    if dependency not in tracked or not dependency.is_file():
                        raise ValueError(f"JSON 依赖缺失：{source} -> {ref.path}")
                    attachments.add(dependency)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pandoc", default="pandoc", help="Pandoc 可执行文件路径")
    parser.add_argument("--output", type=Path, required=True, help="不存在或为空的输出目录")
    parser.add_argument("--cache", type=Path, default=ROOT / "target/documentation-cache")
    args = parser.parse_args()
    output = args.output.resolve()
    if output.exists() and any(output.iterdir()):
        raise ValueError("文档输出目录必须为空，以免打包旧文件")
    output.mkdir(parents=True, exist_ok=True)
    documents = public_documents()
    tracked = {
        (ROOT / path).resolve()
        for path in subprocess.check_output(
            ["git", "ls-files", "-z"], cwd=ROOT,
        ).decode("utf-8").split("\0") if path
    }
    if not set(documents).issubset(tracked):
        raise ValueError("公开文档清单包含未跟踪文件")
    assets = Path("manual/_assets")
    (output / assets).mkdir(parents=True, exist_ok=True)
    copy_css(args.cache, output / assets)
    attachments = set()
    for source in documents:
        document = json.loads(run_pandoc(args.pandoc, "--from=gfm", "--to=json", source))
        rewrite_links(document, source, documents, attachments, tracked)
        page = destination(source)
        (output / page).parent.mkdir(parents=True, exist_ok=True)
        heading = next(node for node in nodes(document) if node.get("t") == "Header")
        title = run_pandoc(
            args.pandoc, "--from=json", "--to=plain", "--wrap=none",
            content=json.dumps({**document, "blocks": [{"t": "Plain", "c": heading["c"][2]}]}),
        ).strip()
        if title != "HMI Compiler":
            title += " · HMI Compiler"
        run_pandoc(
            args.pandoc, "--from=json", "--to=html5", "--standalone", "--wrap=none",
            "--syntax-highlighting=none", f"--template={SUPPORT / 'template.html'}",
            f"--metadata=title:{title}",
            f"--variable=home:{relative_url(Path('README.html'), page)}",
            f"--variable=assets:{relative_url(assets, page)}",
            f"--output={output / page}", content=json.dumps(document),
        )
    copy_attachments(attachments, tracked, output)
    pages, links = validate_html(output)
    print(f"HTML 文档生成完成：{pages} 页，{links} 个本地链接检查通过。入口：{output / 'README.html'}")


if __name__ == "__main__":
    main()
