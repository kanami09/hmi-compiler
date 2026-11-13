"""生成数字、虚拟浮点数、选择与进度控件的分批验收工程。"""

import argparse
from datetime import date
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--font", type=Path, required=True, help="包含数字、负号和小数点的 UTF-8 ZI 字库，建议字高 68")
    parser.add_argument("--output", type=Path, default=root / "target/acceptance-numeric")
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
    for tag, profile, width, height in [
        ("K035", "TJC4832K035_011", 320, 480), ("X543", "TJC4827X543_011", 480, 272)
    ]:
        project = output / f"project-{tag}"
        run("hmi", "init", project, "--target", profile)
        seed = project / "pages/page0/page.toml"
        seed.unlink()
        seed.parent.rmdir()
        assets = project / "assets"
        assets.mkdir()
        shutil.copyfile(font, assets / "ui.zi")
        (assets / "ui.asset.toml").write_text('schema="hmi.asset/1"\nkind="font"\nsource="ui.zi"\n', encoding="utf-8")
        (project / "hmi.toml").write_text(f'''schema="hmi.project/1"
name="Indicators{tag}"
[target]
profile="{profile}"
format="ver21234"
encoding="utf-8"
orientation=0
[program]
source="program.s"
[pages]
sources=["pages/numbers/page.toml","pages/choices/page.toml","pages/progress/page.toml"]
[assets]
items=[{{key="ui",source="assets/ui.asset.toml"}}]
''', encoding="utf-8")
        for name in ["numbers", "choices", "progress"]:
            (project / "pages" / name).mkdir(parents=True)
        (project / "pages/numbers/page.toml").write_text('''schema="hmi.page/1"
name="numbers"
[[objects]]
name="n0"
type="number"
properties={x=10,y=10,w=300,h=70,font={ref="ui"},val=-12}
events={up="next.up.s"}
[[objects]]
name="x0"
type="xfloat"
properties={x=10,y=90,w=300,h=70,font={ref="ui"},val=-1234,vvs1=2}
[[objects]]
name="x1"
type="xfloat"
properties={x=10,y=170,w=300,h=70,font={ref="ui"},val=-1234,vvs1=3,style=1}
''', encoding="utf-8")
        (project / "pages/numbers/next.up.s").write_text("page choices\n", encoding="utf-8")
        (project / "pages/choices/page.toml").write_text('''schema="hmi.page/1"
name="choices"
[[objects]]
name="c0"
type="checkbox"
properties={x=20,y=20,w=48,h=48,val=0}
events={down="select.down.s",up="select.up.s"}
[[objects]]
name="c1"
type="checkbox"
properties={x=120,y=20,w=48,h=48,val=1}
[[objects]]
name="r0"
type="radio"
properties={x=20,y=100,w=48,h=48,val=0}
[[objects]]
name="r1"
type="radio"
properties={x=120,y=100,w=48,h=48,val=1}
''', encoding="utf-8")
        (project / "pages/choices/select.down.s").write_text("sys0=1\n", encoding="utf-8")
        (project / "pages/choices/select.up.s").write_text("sys0=2\npage progress\n", encoding="utf-8")
        (project / "pages/progress/page.toml").write_text('''schema="hmi.page/1"
name="progress"
[[objects]]
name="j0"
type="progress_bar"
properties={x=20,y=20,w=280,h=40,val=40}
events={up="next.up.s"}
[[objects]]
name="j1"
type="progress_bar"
properties={x=20,y=90,w=40,h=150,val=75,dez=1,dis=0}
''', encoding="utf-8")
        (project / "pages/progress/next.up.s").write_text("page numbers\n", encoding="utf-8")
        run("hmi", "check", project, "-Werror")
        assert not (project / ".build").exists()
        hmi = output / f"Indicators{tag}.HMI"
        run("hmi", "build", project, "-o", hmi, "-Werror")
        plan = project / ".build/manual"
        run("hmi", "plan", project, "-o", plan)
        jobs = json.loads((plan / "jobs.json").read_bytes())["jobs"]
        for job in jobs:
            run(job["command"], *job["args"])
        manual = project / f"dist/Indicators{tag}/Indicators{tag}.HMI"
        assert hmi.read_bytes() == manual.read_bytes()
        run("hmi-link", plan / "link.toml", "--check", "-Werror")
        inspection = run("hmi-compile", "inspect", plan / "obj/p00000.hmo")
        for i, value in enumerate([-12, -1234, -1234], 1):
            assert inspection["objects"][i]["properties"]["val"] == value
            assert inspection["objects"][i]["properties"]["font"] == {"ref": "ui"}
        assert inspection["objects"][1]["properties"]["lenth"] == 0
        font_info = run("hmi-media", "inspect", plan / "assets/a00000.hma")["metadata"]["font"]
        assert font_info["source_sha256"] == hashlib.sha256(font.read_bytes()).hexdigest()
        cases.append({"file": hmi.name, "sha256": hashlib.sha256(hmi.read_bytes()).hexdigest(),
                      "profile": profile, "orientation": 0, "page_size": [width, height],
                      "font": font_info, "pages": ["numbers", "choices", "progress"],
                      "standalone_jobs": len(jobs), "standalone_equals_driver": True,
                      "opened": None, "compiled": None, "content_verified": None})
    (output / "verification.json").write_text(json.dumps({"generated_on": date.today().isoformat(),
        "stage": "numeric-selection-progress", "automatic_checks_passed": True, "cases": cases},
        ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    font_height = cases[0]["font"]["height"]
    (output / "README.md").write_text(f'''# 数字、选择与进度控件验收

请用 USART HMI 打开 IndicatorsK035.HMI、IndicatorsX543.HMI，逐页核对后分别编译。需要保存时请另存副本；原始摘要在 verification.json。

| 文件 | 配置 |
|---|---|
| IndicatorsK035.HMI | TJC4832K035_011，0°，320×480 |
| IndicatorsX543.HMI | TJC4827X543_011，0°，480×272 |

两份均有 numbers、choices、progress 三页、九个普通控件、一份字高 {font_height} 的字体（ID 0），无图片。字体至少需要覆盖数字、负号和小数点。

## numbers（页面 ID 0）

| 控件／ID | 位置、尺寸 | 属性与预期 |
|---|---|---|
| n0／1 | 10/10，300×70 | number，val=-12，lenth=0，format=0，显示 -12 |
| x0／2 | 10/90，300×70 | xfloat，val=-1234，vvs0=0、vvs1=2，显示 -12.34 |
| x1／3 | 10/170，300×70 | xfloat，val=-1234，vvs0=0、vvs1=3，显示 -1.234，黑色边框，宽度 2 |

均为单色白底黑字、居中对齐。n0、x0 为平面，x1 为边框。n0.up 为 page choices。

## choices（页面 ID 1）

c0（ID 1）、c1（ID 2）为复选框，位置 20/20、120/20，均 48×48，val 分别为 0、1；style=2，边框黑色、宽度 2。

r0（ID 3）、r1（ID 4）为单选框，位置 20/100、120/100，均 48×48，val 分别为 0、1。四个控件均白底、绿色前景 pco=1024。不要求单选框自动互斥，未生成互斥业务代码。

c0.down 为 sys0=1；c0.up 为 sys0=2、page progress，每条各占一行。其他选择控件事件为空。

## progress（页面 ID 2）

j0（ID 1）位于 20/20，280×40，单色 sta=0、横向 dez=0、val=40、dis=25。背景灰色 bco=50712、前景绿色 pco=1024，进度约 40%。此前 K035 显示直角、X543 显示圆角，两者不要求形状一致。

j1（ID 2）位于 20/90，40×150，单色 sta=0、竖向 dez=1、val=75、dis=0，颜色同上，进度约 75%。本批需特别核对竖向显示及方向属性是否正确。j0.up 为 page numbers。

请反馈两份是否正常打开、负数和小数位是否正确、选择状态与进度显示是否符合说明，以及编译结果和提示。当前不要求模拟器或硬件运行。

源码位于 project-K035／project-X543。每份实际执行 6 条独立工具命令，输出与统一入口完全相同，独立链接检查通过。
''', encoding="utf-8")
    print(f"数字与进度控件验收工程已生成：{output}")


if __name__ == "__main__":
    main()
