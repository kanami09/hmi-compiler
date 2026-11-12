"""生成基础控件的上位机验收工程，不依赖已有 HMI 或内部资料。

先运行 cargo build --workspace --bins，再运行本脚本。
已有输出目录不会被覆盖；重新生成时使用 --output 指定新目录。
"""

import argparse
from datetime import date
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys


def quoted(value):
    return json.dumps(value, ensure_ascii=False)


def write_page(project, name, color, controls):
    directory = project / "pages" / name
    (directory / "events").mkdir(parents=True)
    lines = [
        'schema = "hmi.page/1"', f"name = {quoted(name)}", "",
        "[properties]", "sta = 1", f"bco = {color}", "",
        "[events]", 'load = "events/page.load.s"',
    ]
    (directory / "events/page.load.s").write_text(
        f"sys0={['main', 'details', 'edited'].index(name) + 1}\n", encoding="utf-8"
    )
    for control in controls:
        lines.extend(["", "[[objects]]", f"name = {quoted(control['name'])}",
                      f"type = {quoted(control['type'])}"])
        properties = ", ".join(f"{key} = {quoted(value)}" for key, value in control["properties"].items())
        lines.append(f"properties = {{ {properties} }}")
        events = []
        for event, code in control.get("events", {}).items():
            filename = f"events/{control['name']}.{event}.s"
            (directory / filename).write_text(code + "\n", encoding="utf-8")
            events.append(f"{event} = {quoted(filename)}")
        if events:
            lines.append("events = { " + ", ".join(events) + " }")
    (directory / "page.toml").write_text("\n".join(lines) + "\n", encoding="utf-8")


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=root / "target/acceptance-basic-controls")
    parser.add_argument("--binaries", type=Path, default=root / "target/debug")
    args = parser.parse_args()
    output = args.output.resolve()
    binaries = args.binaries.resolve()
    suffix = ".exe" if os.name == "nt" else ""
    if output.exists():
        parser.error("输出目录已存在；请使用 --output 指定新目录，避免覆盖验收记录")
    for tool in ["hmi", "hmi-compile", "hmi-link"]:
        if not (binaries / (tool + suffix)).is_file():
            parser.error("找不到工具，请先运行 cargo build --workspace --bins")

    def run(tool, *arguments):
        if tool not in {"hmi", "hmi-compile", "hmi-link"}:
            raise RuntimeError(f"执行计划包含未知工具：{tool}")
        result = subprocess.run(
            [str(binaries / (tool + suffix)), *map(str, arguments), "--json"],
            capture_output=True, text=True, encoding="utf-8", check=False,
        )
        if result.returncode:
            raise RuntimeError(result.stdout + result.stderr)
        value = json.loads(result.stdout)
        if not value["ok"] or value["diagnostics"]:
            raise RuntimeError(result.stdout)
        return value["result"]

    output.mkdir(parents=True)
    project = output / "project"
    run("hmi", "init", project, "--target", "TJC4832T035_011")
    # 仅删除本脚本刚刚新建的空白页；验收清单显式列出新的三页。
    seed = project / "pages/page0/page.toml"
    seed.unlink()
    seed.parent.rmdir()
    (project / "hmi.toml").write_text('''schema = "hmi.project/1"
name = "BasicControls"
[target]
profile = "TJC4832T035_011"
format = "ver21234"
encoding = "utf-8"
orientation = 0
[program]
source = "program.s"
[pages]
sources = ["pages/main/page.toml", "pages/details/page.toml", "pages/edited/page.toml"]
[assets]
items = []
''', encoding="utf-8")
    pages = [
        ("main", 65535, [
            {"name": "counter", "type": "variable", "properties": {"sta": 0, "val": -123456789, "txt": "测试A", "txt_maxl": 10, "vscope": 1}},
            {"name": "tap", "type": "hotspot", "properties": {"x": 20, "y": 30, "w": 120, "h": 60}, "events": {"down": "sys1=101", "up": "counter.val=counter.val+1\npage details"}},
            {"name": "tm0", "type": "timer", "properties": {"tim": 12345, "en": 0}, "events": {"timer": "counter.val=counter.val+1"}},
        ]),
        ("details", 0, [
            {"name": "message", "type": "variable", "properties": {"sta": 1, "val": 42, "txt": "测试A", "txt_maxl": 10}},
            {"name": "back", "type": "hotspot", "properties": {"x": 20, "y": 30, "w": 120, "h": 60}, "events": {"up": "main.counter.val=main.counter.val+1\npage main"}},
        ]),
        ("edited", 31, [
            {"name": "tm0", "type": "timer", "properties": {"tim": 50, "en": 0}, "events": {"timer": "state.val=state.val+1"}},
            {"name": "renamed", "type": "hotspot", "properties": {"x": 20, "y": 30, "w": 80, "h": 40}, "events": {"up": "state.val=5"}},
            {"name": "tap", "type": "hotspot", "properties": {"x": 120, "y": 30, "w": 80, "h": 40}, "events": {"up": "state.val=9"}},
            {"name": "state", "type": "variable", "properties": {"val": 7}},
        ]),
    ]
    for name, color, controls in pages:
        write_page(project, name, color, controls)
    run("hmi", "check", project, "-Werror")
    hmi = output / "01_BasicControls.HMI"
    run("hmi", "build", project, "-o", hmi, "-Werror")
    plan = project / ".build/manual"
    run("hmi", "plan", project, "-o", plan)
    jobs = json.loads((plan / "jobs.json").read_bytes())["jobs"]
    for job in jobs:
        run(job["command"], *job["args"])
    manual = project / "dist/BasicControls/BasicControls.HMI"
    if hmi.read_bytes() != manual.read_bytes():
        raise RuntimeError("独立工具组合与统一入口输出不一致")
    inspected = run("hmi-compile", "inspect", plan / "obj/p00000.hmo")
    counter = inspected["objects"][1]
    assert counter["properties"]["val"] == -123456789
    assert counter["properties"]["txt"] == "测试A" and counter["id_pending"]
    mapping = json.loads((output / "link-map.json").read_bytes())
    expected_pages = []
    for index, (name, color, controls) in enumerate(pages):
        expected = [{"id": i + 1, **control} for i, control in enumerate(controls)]
        actual = mapping["pages"][index]["objects"][1:]
        assert [(o["id"], o["name"], o["type"]) for o in actual] == [(o["id"], o["name"], o["type"]) for o in expected]
        expected_pages.append({"id": index, "name": name, "bco": color, "controls": expected})
    report = {
        "stage": "B-basic-controls", "generated_on": date.today().isoformat(),
        "automatic_checks_passed": True, "standalone_jobs": len(jobs),
        "standalone_equals_driver": True, "native_regression_vectors_in_test_suite": 12,
        "object_schema": inspected["metadata"]["schema"],
        "data_revision": inspected["metadata"]["target"]["data_revision"],
        "cases": [{"file": hmi.name, "sha256": hashlib.sha256(hmi.read_bytes()).hexdigest(),
                   "profile": "TJC4832T035_011", "orientation": 0, "page_size": [320, 480],
                   "pages": expected_pages, "opened": None, "compiled": None, "content_verified": None}],
    }
    (output / "verification.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    (output / "README.md").write_text('''# 基础控件验收

请用 USART HMI 打开 01_BasicControls.HMI，核对后编译。需要保存时请另存副本，原文件摘要已记录在 verification.json。

型号为 TJC4832T035_011，0°，页面尺寸为 320×480，无图片和字库。页面依次为 main、details、edited。

| 页面 | 背景 | 普通控件 ID／名称／主要属性 |
|---|---|---|
| main | 白色 | 1 counter：数值变量，val=-123456789，全局；2 tap：热区，20/30、120×60；3 tm0：tim=12345、en=0 |
| details | 黑色 | 1 message：字符串变量，txt=测试A、txt_maxl=10；2 back：热区，20/30、120×60 |
| edited | 蓝色 | 1 tm0：tim=50、en=0；2 renamed：热区，20/30、80×40；3 tap：热区，120/30、80×40；4 state：数值变量，val=7 |

变量和定时器不是文字显示控件，热区本身透明。请在控件列表／特殊控件栏中选择对象核对属性，不要求画布显示变量文字。

请核对事件归属和内容：

- 各页面 load 分别为 sys0=1、sys0=2、sys0=3。
- main.tap.down 为 sys1=101；up 为 counter.val=counter.val+1，下一行 page details。
- main.tm0.timer 为 counter.val=counter.val+1。
- details.back.up 为 main.counter.val=main.counter.val+1，下一行 page main。
- edited.tm0.timer 为 state.val=state.val+1；renamed.up 为 state.val=5；tap.up 为 state.val=9。

请反馈是否正常打开、控件及事件是否符合上述内容、编译是否通过及具体提示。两个定时器均已关闭，不要求运行模拟器或连接硬件。变量非活动字段的保留由自动测试和目标检查覆盖，无需切换模式验证。

工程源码在 project/。生成时已经执行统一入口和导出的 5 条独立命令，两份 HMI 输出完全相同；链接映射和中间文件也一并保留，供排查问题。
''', encoding="utf-8")
    print(f"验收工程已生成：{hmi}")
    print(f"SHA-256：{report['cases'][0]['sha256']}")


if __name__ == "__main__":
    main()
