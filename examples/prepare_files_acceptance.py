"""生成 X543 文件浏览器与未打开文件流验收工程，不访问虚拟 SD。"""
import argparse
from datetime import date
import hashlib
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import zlib


def icon_png(folder):
    pixels = bytearray(64 * 64 * 4)
    def rect(left, top, right, bottom, color):
        for y in range(top, bottom):
            for x in range(left, right):
                offset = (y * 64 + x) * 4
                pixels[offset:offset + 4] = bytes((*color, 255))
    if folder:
        rect(8, 10, 30, 23, (140, 95, 20))
        rect(10, 12, 28, 23, (250, 195, 60))
        rect(6, 19, 58, 53, (140, 95, 20))
        rect(8, 21, 56, 51, (250, 195, 60))
    else:
        rect(15, 6, 49, 58, (70, 90, 120))
        rect(17, 8, 47, 56, (235, 242, 255))
        rect(35, 8, 47, 20, (90, 160, 235))
        for y in [28, 35, 42]:
            rect(23, y, 41, y + 3, (70, 120, 180))
    raw = b"".join(b"\0" + pixels[y * 256:(y + 1) * 256] for y in range(64))
    def chunk(tag, data):
        return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", zlib.crc32(tag + data))
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", 64, 64, 8, 6, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b""))


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--font", type=Path, required=True, help="UTF-8 ZI 字库，建议使用已验收的字库")
    parser.add_argument("--output", type=Path, default=root / "target/acceptance-files")
    parser.add_argument("--binaries", type=Path, default=root / "target/debug")
    args = parser.parse_args()
    output, binaries, font = args.output.resolve(), args.binaries.resolve(), args.font.resolve()
    if output.exists():
        parser.error("输出目录已存在，请使用 --output 指定新目录")
    if not font.is_file():
        parser.error("请提供已有的独立 ZI 字库")
    suffix = ".exe" if os.name == "nt" else ""
    tools = {"hmi", "hmi-compile", "hmi-media", "hmi-link"}
    if any(not (binaries / (tool + suffix)).is_file() for tool in tools):
        parser.error("请先运行 cargo build --workspace --bins")

    def run(tool, *arguments):
        if tool not in tools:
            raise RuntimeError(f"执行计划包含未知工具：{tool}")
        result = subprocess.run([str(binaries / (tool + suffix)), *map(str, arguments), "--json"],
                                capture_output=True, text=True, encoding="utf-8", check=False)
        if result.returncode:
            raise RuntimeError(result.stdout + result.stderr)
        value = json.loads(result.stdout)
        if not value["ok"] or value["diagnostics"]:
            raise RuntimeError(result.stdout)
        return value["result"]

    output.mkdir(parents=True)
    cases = []
    for variant in [0, 1]:
        project = output / f"project-{variant}"
        run("hmi", "init", project, "--target", "TJC4827X543_011")
        seed = project / "pages/page0/page.toml"
        seed.unlink()
        seed.parent.rmdir()
        assets = project / "assets"
        assets.mkdir()
        shutil.copyfile(font, assets / "ui.zi")
        (assets / "ui.asset.toml").write_text('schema="hmi.asset/1"\nkind="font"\nsource="ui.zi"\n', encoding="utf-8")
        for name, folder in [("folder", True), ("file", False)]:
            (assets / f"{name}.png").write_bytes(icon_png(folder))
            (assets / f"{name}.asset.toml").write_text(f'schema="hmi.asset/1"\nkind="image"\nsource="{name}.png"\n[encoding]\nquality=96\n', encoding="utf-8")
        (project / "hmi.toml").write_text(f'''schema="hmi.project/1"
name="Files{variant}"
[target]
profile="TJC4827X543_011"
format="ver21234"
encoding="utf-8"
orientation=0
[program]
source="program.s"
[pages]
sources=["pages/browser/page.toml","pages/stream/page.toml"]
[assets]
items=[{{key="ui",source="assets/ui.asset.toml"}},{{key="folder",source="assets/folder.asset.toml"}},{{key="file",source="assets/file.asset.toml"}}]
''', encoding="utf-8")
        for page in ["browser", "stream"]:
            (project / "pages" / page).mkdir(parents=True)
        pattern = "*.txt" if variant == 0 else "*.txt|*.data"
        (project / "pages/browser/page.toml").write_text(f'''schema="hmi.page/1"
name="browser"
[[objects]]
name="fb0"
type="file_browser"
properties={{x=20,y=20,w=440,h=232,font={{ref="ui"}},pic1={{ref="folder"}},pic2={{ref="file"}},dir="sd0/",filter="{pattern}"}}
events={{up="fb0.up.s"}}
''', encoding="utf-8")
        (project / "pages/browser/fb0.up.s").write_text("sys0=fb0.val\n", encoding="utf-8")
        (project / "pages/stream/page.toml").write_text(f'''schema="hmi.page/1"
name="stream"
[[objects]]
name="fs{variant}"
type="file_stream"
''', encoding="utf-8")
        run("hmi", "check", project, "-Werror")
        assert not (project / ".build").exists()
        hmi = output / f"Files{variant}.HMI"
        run("hmi", "build", project, "-o", hmi, "-Werror")
        plan = project / ".build/manual"
        run("hmi", "plan", project, "-o", plan)
        jobs = json.loads((plan / "jobs.json").read_bytes())["jobs"]
        for job in jobs:
            run(job["command"], *job["args"])
        manual = project / f"dist/Files{variant}/Files{variant}.HMI"
        assert hmi.read_bytes() == manual.read_bytes()
        run("hmi-link", plan / "link.toml", "--check", "-Werror")
        fb = run("hmi-compile", "inspect", plan / "obj/p00000.hmo")["objects"][1]
        assert fb["properties"]["pic1"] == {"ref": "folder"}
        assert fb["properties"]["pic2"] == {"ref": "file"}
        stream = run("hmi-compile", "inspect", plan / "obj/p00001.hmo")["objects"][1]
        assert stream["name"] == f"fs{variant}" and "x" not in stream["properties"]
        for index in [1, 2]:
            info = run("hmi-media", "inspect", plan / f"assets/a{index:05}.hma")["metadata"]["image"]
            assert info["jpeg"]["has_alpha"]
        cases.append({"file": hmi.name, "sha256": hashlib.sha256(hmi.read_bytes()).hexdigest(),
                      "profile": "TJC4827X543_011", "orientation": 0, "page_size": [480, 272],
                      "filter": pattern, "stream_name": f"fs{variant}", "icon_ids": {"folder": 0, "file": 1},
                      "standalone_jobs": len(jobs), "standalone_equals_driver": True,
                      "opened": None, "compiled": None, "content_verified": None})
    (output / "verification.json").write_text(json.dumps({"generated_on": date.today().isoformat(),
        "stage": "file-browser-stream", "automatic_checks_passed": True, "cases": cases},
        ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    (output / "README.md").write_text('''# 文件浏览器与文件流验收

请用 USART HMI 打开 Files0.HMI、Files1.HMI，逐页核对后分别编译。需要保存时请另存副本，原始摘要见 verification.json。

两份均为 TJC4827X543_011、0°、480×272，browser／stream 两页、各页一个普通控件（ID 1），字体 ID 0。图标在工程中显式声明，图片 ID 0 为黄色文件夹，ID 1 为蓝白文件图标，均为 64×64 透明 PNG 转换的显示资源。

## browser（页面 ID 0）

fb0 位于 20/20、440×232，私有、单色白底黑字，style=2、边框颜色 0、宽度 2。font=0、pic1=0、pic2=1，dir 为 sd0/。

- Files0.HMI：filter 为 *.txt。
- Files1.HMI：filter 为 *.txt|*.data。

psta=0、buffsize=32000、fwarning=1；xcen=1、left=1、ch=8、dis=3、spax=24、spay=8；选中背景 bco2=31，选中文字 pco2=65535。up 事件为 sys0=fb0.val。

构建时没有遍历 sd0/ 或填入文件列表；打开后的内容依赖你现有的虚拟 SD 环境，不要求固定文件名、目录或条数，也不要求修改、清空或创建虚拟 SD 文件。文本显示受所选字库字符覆盖范围影响。请核对图标资源是否存在、属性和编译结果；若出现 UTF-8 警告，请保留具体提示，不必先关闭 fwarning。

## stream（页面 ID 1）

Files0.HMI 中为 fs0，Files1.HMI 中为 fs1。文件流在特殊控件栏，无可见几何、无事件，vscope=0，初始未打开文件，val=0、qty=0、en=0。qty/en 只读；原生方法与保留字段不需要手动修改。

本批核对控件配置和编译，不要求打开／读写 SD 文件、运行方法、启动模拟器或连接硬件。数据记录控件及 .data 配套文件在后续阶段接入。

请反馈两份是否正常打开、图标和控件属性是否符合说明，以及编译结果和提示。源码在 project-0／project-1；每份实际执行 7 条独立命令，输出与统一入口相同，独立链接检查通过。脚本只使用独立字库和自行生成的图标，不读取已有 HMI、内部素材或虚拟 SD。
''', encoding="utf-8")
    print(f"文件控件验收工程已生成：{output}")


if __name__ == "__main__":
    main()
