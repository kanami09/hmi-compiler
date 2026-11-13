"""使用两份独立 ZI 字库生成文本／按钮验收工程，不读取已有 HMI。"""

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
    parser.add_argument("--font-small", required=True, type=Path, help="包含 ABC012 的小字库")
    parser.add_argument("--font-large", required=True, type=Path, help="包含 ABC012 的大字库")
    parser.add_argument("--output", type=Path, default=root / "target/acceptance-fonts")
    parser.add_argument("--binaries", type=Path, default=root / "target/debug")
    args = parser.parse_args()
    output, binaries = args.output.resolve(), args.binaries.resolve()
    if output.exists():
        parser.error("输出目录已存在，请使用 --output 指定新目录")
    fonts = {"small": args.font_small.resolve(), "large": args.font_large.resolve()}
    if any(not path.is_file() for path in fonts.values()):
        parser.error("请提供两份已有的独立 ZI 字库")
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
    for orientation, profile, width, height in [
        (0, "TJC4832T035_011", 320, 480), (90, "TJC4832K035_011", 480, 320)
    ]:
        project = output / f"project-{orientation}"
        run("hmi", "init", project, "--target", profile, "--orientation", orientation)
        seed = project / "pages/page0/page.toml"
        seed.unlink()
        seed.parent.rmdir()
        assets = project / "assets"
        assets.mkdir()
        for key, source in fonts.items():
            shutil.copyfile(source, assets / f"{key}.zi")
            (assets / f"{key}.asset.toml").write_text(
                f'schema="hmi.asset/1"\nkind="font"\nsource="{key}.zi"\n', encoding="utf-8")
        order = ["large", "small"] if orientation == 0 else ["small", "large"]
        items = ",".join('{key="' + key + '",source="assets/' + key + '.asset.toml"}' for key in order)
        (project / "hmi.toml").write_text(f'''schema="hmi.project/1"
name="TextFonts{orientation}"
[target]
profile="{profile}"
format="ver21234"
encoding="utf-8"
orientation={orientation}
[program]
source="program.s"
[pages]
sources=["pages/overview/page.toml","pages/detail/page.toml"]
[assets]
items=[{items}]
''', encoding="utf-8")
        for name in ["overview", "detail"]:
            (project / "pages" / name).mkdir(parents=True)
        (project / "pages/overview/page.toml").write_text('''schema="hmi.page/1"
name="overview"
[[objects]]
name="t_large"
type="text"
properties={x=10,y=10,w=300,h=100,font={ref="large"},txt="ABC012"}
[[objects]]
name="t_small"
type="text"
properties={x=10,y=120,w=300,h=50,font={ref="small"},txt="ABC012",style=1}
[[objects]]
name="b_next"
type="button"
properties={x=10,y=190,w=220,h=70,font={ref="small"},txt="ABC012"}
events={down="next.down.s",up="next.up.s"}
''', encoding="utf-8")
        (project / "pages/overview/next.down.s").write_text("sys0=1\n", encoding="utf-8")
        (project / "pages/overview/next.up.s").write_text("sys0=2\npage detail\n", encoding="utf-8")
        (project / "pages/detail/page.toml").write_text('''schema="hmi.page/1"
name="detail"
[[objects]]
name="t_status"
type="text"
properties={x=10,y=10,w=300,h=80,font={ref="small"},txt="ABC012",style=1,bco=0,pco=65535,borderc=63488}
[[objects]]
name="b_back"
type="button"
properties={x=10,y=120,w=220,h=70,font={ref="small"},txt="ABC012",style=1,bco=65535}
events={up="back.up.s"}
''', encoding="utf-8")
        (project / "pages/detail/back.up.s").write_text('t_status.txt="ABC"\npage overview\n', encoding="utf-8")
        run("hmi", "check", project, "-Werror")
        assert not (project / ".build").exists()
        hmi = output / f"TextFonts{orientation}.HMI"
        run("hmi", "build", project, "-o", hmi, "-Werror")
        plan = project / ".build/manual"
        run("hmi", "plan", project, "-o", plan)
        jobs = json.loads((plan / "jobs.json").read_bytes())["jobs"]
        for job in jobs:
            run(job["command"], *job["args"])
        manual = project / f"dist/TextFonts{orientation}/TextFonts{orientation}.HMI"
        assert hmi.read_bytes() == manual.read_bytes()
        run("hmi-link", plan / "link.toml", "--check", "-Werror")
        inspection = run("hmi-compile", "inspect", plan / "obj/p00000.hmo")
        assert inspection["objects"][1]["properties"]["font"] == {"ref": "large"}
        assert inspection["objects"][3]["properties"]["font"] == {"ref": "small"}
        font_info = {}
        for i, key in enumerate(order):
            info = run("hmi-media", "inspect", plan / f"assets/a{i:05}.hma")["metadata"]["font"]
            assert info["orientation"] == orientation
            assert hashlib.sha256((assets / f"{key}.zi").read_bytes()).hexdigest() == info["source_sha256"]
            font_info[key] = {"id": i, **info}
        cases.append({"file": hmi.name, "sha256": hashlib.sha256(hmi.read_bytes()).hexdigest(),
                      "profile": profile, "orientation": orientation, "page_size": [width, height],
                      "fonts": font_info, "standalone_jobs": len(jobs), "standalone_equals_driver": True,
                      "opened": None, "compiled": None, "content_verified": None})
    (output / "verification.json").write_text(json.dumps({"generated_on": date.today().isoformat(),
        "stage": "zi-text-button", "automatic_checks_passed": True, "cases": cases},
        ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    small_height, large_height = (cases[0]["fonts"][key]["height"] for key in ["small", "large"])
    (output / "README.md").write_text(f'''# 字库、文本和按钮验收

请用 USART HMI 打开 TextFonts0.HMI 和 TextFonts90.HMI，核对内容后分别编译。需要保存时请另存副本；原始摘要在 verification.json。

| 文件 | 型号、方向与页面尺寸 | 字体 ID |
|---|---|---|
| TextFonts0.HMI | TJC4832T035_011，0°，320×480 | 0：大字库，高 {large_height}；1：小字库，高 {small_height} |
| TextFonts90.HMI | TJC4832K035_011，90°，480×320 | 0：小字库，高 {small_height}；1：大字库，高 {large_height} |

两份均有 overview、detail 两页，全部初始文字为 ABC012。文字应相对当前页面正常排列，无旋转、倒置或乱码。两份刻意交换字库顺序，控件应仍使用正确字高。

overview：t_large（ID 1）位于 10/10，300×100，大字库、白底黑字、平面；t_small（ID 2）位于 10/120，300×50，小字库、白底黑字、黑色边框；b_next（ID 3）位于 10/190，220×70，小字库、默认 3D_Auto 风格。三个控件都居中对齐、文本容量 10。

detail：t_status（ID 1）位于 10/10，300×80，小字库、黑底白字、红色边框；b_back（ID 2）位于 10/120，220×70，小字库、白色普通背景、边框风格。边框宽度均为 2。

核对事件代码：b_next.down 为 sys0=1；b_next.up 为 sys0=2、page detail；b_back.up 为 t_status.txt="ABC"、page overview，每条各占一行。当前不要求模拟器或硬件运行。

请反馈两份能否正常打开、字库列表与字高是否正确、文字／控件风格与方向是否符合说明，以及编译结果和提示。源工程位于 project-0／project-90；每份实际执行 6 条独立工具命令，输出与统一入口完全相同。
''', encoding="utf-8")
    print(f"字库与文本验收工程已生成：{output}")


if __name__ == "__main__":
    main()
