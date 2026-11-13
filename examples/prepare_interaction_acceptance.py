"""使用独立 ZI 字库生成滑块、双态按钮、滚动文本和触摸捕捉验收工程。"""

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
    parser.add_argument("--font", type=Path, required=True, help="包含示例 ASCII 字符的 UTF-8 ZI 字库，建议字高 68")
    parser.add_argument("--output", type=Path, default=root / "target/acceptance-interaction")
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
        pages = ["sliders", "buttons", "scroll", "capture"]
        sources = ",".join(f'"pages/{name}/page.toml"' for name in pages)
        (project / "hmi.toml").write_text(f'''schema="hmi.project/1"
name="Interaction{tag}"
[target]
profile="{profile}"
format="ver21234"
encoding="utf-8"
orientation=0
[program]
source="program.s"
[pages]
sources=[{sources}]
[assets]
items=[{{key="ui",source="assets/ui.asset.toml"}}]
''', encoding="utf-8")
        for name in pages:
            (project / "pages" / name).mkdir(parents=True)
        cursor_mode = 2 if tag == "X543" else 0
        (project / "pages/sliders/page.toml").write_text('''schema="hmi.page/1"
name="sliders"
[[objects]]
name="h0"
type="slider"
properties={x=20,y=20,w=280,h=40,val=40,minval=0,maxval=100}
events={down="h0.down.s",up="h0.up.s",slide="h0.slide.s"}
[[objects]]
name="h1"
type="slider"
properties={x=20,y=90,w=40,h=150,mode=1,psta=0,wid=20,hig=20,val=75,minval=0,maxval=100,dis=0}
'''.replace("psta=0", f"psta={cursor_mode}"), encoding="utf-8")
        for slot, code in [("down", "sys0=1"), ("up", "sys0=2"), ("slide", "sys0=3")]:
            (project / f"pages/sliders/h0.{slot}.s").write_text(code + "\n", encoding="utf-8")
        (project / "pages/buttons/page.toml").write_text('''schema="hmi.page/1"
name="buttons"
[[objects]]
name="bt0"
type="dual_state_button"
properties={x=20,y=20,w=280,h=80,font={ref="ui"},txt="DUAL",val=0}
events={down="bt0.down.s",up="bt0.up.s"}
[[objects]]
name="bt1"
type="dual_state_button"
properties={x=20,y=130,w=280,h=80,font={ref="ui"},txt="DUAL",val=1}
''', encoding="utf-8")
        (project / "pages/buttons/bt0.down.s").write_text("sys0=1\n", encoding="utf-8")
        (project / "pages/buttons/bt0.up.s").write_text("sys0=2\n", encoding="utf-8")
        (project / "pages/scroll/page.toml").write_text('''schema="hmi.page/1"
name="scroll"
[[objects]]
name="g0"
type="scrolling_text"
properties={x=20,y=20,w=280,h=80,font={ref="ui"},txt="SCROLL ABC 123"}
events={up="g0.up.s"}
[[objects]]
name="g1"
type="scrolling_text"
properties={x=20,y=130,w=280,h=80,font={ref="ui"},txt="ABC 123",txt_maxl=32,dir=3,dis=2,tim=160,en=0}
''', encoding="utf-8")
        (project / "pages/scroll/g0.up.s").write_text("g0.en=0\n", encoding="utf-8")
        (project / "pages/capture/page.toml").write_text('''schema="hmi.page/1"
name="capture"
[[objects]]
name="tc0"
type="touch_cap"
events={down="tc0.down.s",up="tc0.up.s"}
[[objects]]
name="m0"
type="hotspot"
properties={x=20,y=20,w=120,h=80}
[[objects]]
name="n0"
type="number"
properties={x=20,y=120,w=280,h=70,font={ref="ui"},val=0}
''', encoding="utf-8")
        (project / "pages/capture/tc0.down.s").write_text("sys1=1\n", encoding="utf-8")
        (project / "pages/capture/tc0.up.s").write_text("n0.val=tc0.val\n", encoding="utf-8")
        run("hmi", "check", project, "-Werror")
        assert not (project / ".build").exists()
        hmi = output / f"Interaction{tag}.HMI"
        run("hmi", "build", project, "-o", hmi, "-Werror")
        plan = project / ".build/manual"
        run("hmi", "plan", project, "-o", plan)
        jobs = json.loads((plan / "jobs.json").read_bytes())["jobs"]
        for job in jobs:
            run(job["command"], *job["args"])
        manual = project / f"dist/Interaction{tag}/Interaction{tag}.HMI"
        assert hmi.read_bytes() == manual.read_bytes()
        run("hmi-link", plan / "link.toml", "--check", "-Werror")
        slider = run("hmi-compile", "inspect", plan / "obj/p00000.hmo")
        assert slider["objects"][1]["properties"]["val"] == 40
        assert slider["objects"][2]["properties"]["mode"] == 1
        assert slider["objects"][2]["properties"]["psta"] == cursor_mode
        capture = run("hmi-compile", "inspect", plan / "obj/p00003.hmo")
        assert capture["objects"][1]["type"] == "touch_cap"
        assert "x" not in capture["objects"][1]["properties"]
        font_info = run("hmi-media", "inspect", plan / "assets/a00000.hma")["metadata"]["font"]
        assert font_info["source_sha256"] == hashlib.sha256(font.read_bytes()).hexdigest()
        cases.append({"file": hmi.name, "sha256": hashlib.sha256(hmi.read_bytes()).hexdigest(),
                      "profile": profile, "orientation": 0, "page_size": [width, height], "font": font_info,
                      "pages": pages, "vertical_slider_psta": cursor_mode, "standalone_jobs": len(jobs), "standalone_equals_driver": True,
                      "opened": None, "compiled": None, "content_verified": None})
    (output / "verification.json").write_text(json.dumps({"generated_on": date.today().isoformat(),
        "stage": "slider-dual-scroll-touch", "automatic_checks_passed": True, "cases": cases},
        ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    (output / "README.md").write_text('''# 滑块、双态按钮、滚动文本和触摸捕捉验收

请用 USART HMI 打开 InteractionK035.HMI、InteractionX543.HMI，核对下述四页后分别编译。需要保存时请另存副本；原始摘要在 verification.json。

| 文件 | 配置 |
|---|---|
| InteractionK035.HMI | TJC4832K035_011，0°，320×480 |
| InteractionX543.HMI | TJC4827X543_011，0°，480×272 |

两份各四页、九个普通控件，一份字体（ID 0），无图片。当前只核对上位机属性、预览和编译，不要求模拟器或硬件运行。

## sliders（页面 ID 0）

| 控件／ID | 位置与尺寸 | 重点属性 |
|---|---|---|
| h0／1 | 20/20，280×40 | mode=0、sta=1、psta=0；val=40，minval=0、maxval=100；wid=hig=255，dis=100、ch=25 |
| h1／2 | 20/90，40×150 | mode=1、sta=1；K035 的 psta=0，X543 的 psta=2；val=75，范围 0～100；wid=hig=20，dis=0、ch=25 |

背景灰色 bco=50712、前景绿色 bco1=1024、游标红色 pco=63488。h0 应横向，h1 应竖向；K035 的 h1 使用 psta=0，X543 的 h1 使用 psta=2 单色四边形，按型号区分。此前 h0 的 psta=0 在 K035 预览为方形、X543 为圆形，不要求两型号游标形状相同。

h0.down、up、slide 分别为 sys0=1、sys0=2、sys0=3；h1 事件为空。

## buttons（页面 ID 1）

bt0（ID 1）位于 20/20、bt1（ID 2）位于 20/130，均 280×80，单色、style=4、文字 DUAL、字体 0、容量 10。val 分别为 0（弹起）、1（按下）；背景为 50712／1024，文字颜色为 0／65535，核对两种状态及外观。

bt0.down 为 sys0=1，bt0.up 为 sys0=2。未生成额外的状态切换代码。

## scroll（页面 ID 2）

g0（ID 1）位于 20/20、g1（ID 2）位于 20/130，均 280×80，单色白底黑字、左对齐、垂直居中。

- g0：txt 为 SCROLL ABC 123，txt_maxl=64，dir=1、dis=4、tim=80、en=1。
- g1：txt 为 ABC 123，txt_maxl=32，dir=3、dis=2、tim=160、en=0。

文字允许按控件宽度裁切，完整内容以 txt 属性核对；静态画布不要求实际滚动。g0.up 为 g0.en=0。

## capture（页面 ID 3）

tc0（ID 1）是触摸捕捉，应能在控件列表中选中；没有 x/y/w/h 或可见外形，val 为只读结果、初始 0。它的 down 为 sys1=1，up 为 n0.val=tc0.val。

m0（ID 2）是 20/20、120×80 的热区，不要求画布上有可见形状；n0（ID 3）是 20/120、280×70 的白底黑字数字控件，字体 0、val=0。只核对属性和事件，不要求触摸后数字实际更新。

请反馈两份能否正常打开、属性和预览是否符合说明、是否有编译提示。源码位于 project-K035／project-X543；每份实际执行 7 条独立命令，输出与统一入口完全相同，独立链接检查通过。
''', encoding="utf-8")
    print(f"交互控件验收工程已生成：{output}")


if __name__ == "__main__":
    main()
