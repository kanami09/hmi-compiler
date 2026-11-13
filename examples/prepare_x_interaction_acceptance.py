"""生成 X543 状态开关、下拉框、选择文本和滑动文本验收工程。"""

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
    parser.add_argument("--font", type=Path, required=True, help="包含示例 ASCII 字符的 ZI 字库，建议字高 68")
    parser.add_argument("--output", type=Path, default=root / "target/acceptance-x-interaction")
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
    for state in [0, 1]:
        project = output / f"project-{state}"
        run("hmi", "init", project, "--target", "TJC4827X543_011")
        seed = project / "pages/page0/page.toml"
        seed.unlink()
        seed.parent.rmdir()
        assets = project / "assets"
        assets.mkdir()
        shutil.copyfile(font, assets / "ui.zi")
        (assets / "ui.asset.toml").write_text('schema="hmi.asset/1"\nkind="font"\nsource="ui.zi"\n', encoding="utf-8")
        pages = ["switches", "combo", "select", "sltext"]
        sources = ",".join(f'"pages/{name}/page.toml"' for name in pages)
        (project / "hmi.toml").write_text(f'''schema="hmi.project/1"
name="XInteraction{state}"
[target]
profile="TJC4827X543_011"
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
        for page in pages:
            (project / "pages" / page).mkdir(parents=True)
        (project / "pages/switches/page.toml").write_text(f'''schema="hmi.page/1"
name="switches"
[[objects]]
name="sw0"
type="switch"
properties={{x=20,y=20,w=320,h=88,font={{ref="ui"}},txt="0/1",val={state}}}
events={{up="sw0.up.s"}}
''', encoding="utf-8")
        (project / "pages/switches/sw0.up.s").write_text("sys0=sw0.val\n", encoding="utf-8")
        (project / "pages/combo/page.toml").write_text(f'''schema="hmi.page/1"
name="combo"
[[objects]]
name="cb0"
type="combo_box"
properties={{x=20,y=20,w=300,h=84,font={{ref="ui"}},path="A\\nB\\nC",path_m=64,txt="INIT",txt_maxl=16,val={state}}}
events={{up="cb0.up.s"}}
''', encoding="utf-8")
        (project / "pages/combo/cb0.up.s").write_text("sys1=cb0.val\n", encoding="utf-8")
        (project / "pages/select/page.toml").write_text(f'''schema="hmi.page/1"
name="select"
[[objects]]
name="select0"
type="text_select"
properties={{x=20,y=20,w=300,h=232,font={{ref="ui"}},path="A\\nB\\nC",path_m=64,val={state}}}
events={{up="select0.up.s"}}
''', encoding="utf-8")
        (project / "pages/select/select0.up.s").write_text("sys2=select0.val\n", encoding="utf-8")
        final_line = "D" if state == 0 else "EE"
        (project / "pages/sltext/page.toml").write_text(f'''schema="hmi.page/1"
name="sltext"
[[objects]]
name="slt0"
type="sliding_text"
properties={{x=20,y=20,w=440,h=232,font={{ref="ui"}},txt="A\\nB\\nC\\n{final_line}",txt_maxl=64,path_m=0}}
events={{up="slt0.up.s"}}
''', encoding="utf-8")
        (project / "pages/sltext/slt0.up.s").write_text("slt0.val_y=0\n", encoding="utf-8")
        run("hmi", "check", project, "-Werror")
        assert not (project / ".build").exists()
        hmi = output / f"XInteraction{state}.HMI"
        run("hmi", "build", project, "-o", hmi, "-Werror")
        plan = project / ".build/manual"
        run("hmi", "plan", project, "-o", plan)
        jobs = json.loads((plan / "jobs.json").read_bytes())["jobs"]
        for job in jobs:
            run(job["command"], *job["args"])
        manual = project / f"dist/XInteraction{state}/XInteraction{state}.HMI"
        assert hmi.read_bytes() == manual.read_bytes()
        run("hmi-link", plan / "link.toml", "--check", "-Werror")
        for page_id in [0, 1, 2]:
            widget = run("hmi-compile", "inspect", plan / f"obj/p{page_id:05}.hmo")["objects"][1]
            assert widget["properties"]["val"] == state
            if page_id in [1, 2]:
                assert widget["properties"]["path"] == "A\r\nB\r\nC"
            if page_id == 1:
                assert widget["properties"]["txt"] == "INIT"
        slide = run("hmi-compile", "inspect", plan / "obj/p00003.hmo")["objects"][1]
        assert slide["properties"]["txt"] == "A\r\nB\r\nC\r\n" + final_line
        cases.append({"file": hmi.name, "sha256": hashlib.sha256(hmi.read_bytes()).hexdigest(),
                      "profile": "TJC4827X543_011", "orientation": 0, "page_size": [480, 272],
                      "selection_value": state, "slide_last_line": final_line,
                      "standalone_jobs": len(jobs), "standalone_equals_driver": True,
                      "opened": None, "compiled": None, "content_verified": None})
    (output / "verification.json").write_text(json.dumps({"generated_on": date.today().isoformat(),
        "stage": "x-interaction", "automatic_checks_passed": True, "cases": cases},
        ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    (output / "README.md").write_text('''# X543 交互控件验收

请用 USART HMI 打开 XInteraction0.HMI 和 XInteraction1.HMI，逐页核对后分别编译。需要保存时请另存副本，原始摘要见 verification.json。

两份型号均为 TJC4827X543_011，0°、480×272，各四页、每页一个普通控件（ID 1），共用字体 ID 0。当前核对属性、静态预览和编译；展开选择、滑动手势和事件运行分别验收。

## switches（页面 ID 0）

sw0 位于 20/20、320×88，dez=0，字体 0，文字 0/1、容量 24；两份 val 分别为 0、1。默认状态背景 bco/bco2 为 50712／1024，圆心 pco/pco2 均为 65535，文字色 pco1=0，dis=100。核对两个状态的预览。up 为 sys0=sw0.val。

## combo（页面 ID 1）

cb0 位于 20/20、300×84，单色白底黑字、字体 0、居中。path 为 A、B、C 三行，path_m=64；txt 明确设为 INIT、txt_maxl=16；两份 val 分别为 0、1。构建时未根据 val 把 txt 改成 A 或 B，请分别记录 txt 属性和画布实际显示内容，不要求编辑器一定自动同步。

hig=72、qty=2、dir=1（向下）、down=0（收缩）；up=1 显示箭头，mode=2、wid=12，单元格间距 vvs0=8、标识间距 vvs1=2，ch=8。普通单元格白底黑字，选中背景 bco2=31、文字 pco2=65535。up 事件为 sys1=cb0.val。

## select（页面 ID 2）

select0 位于 20/20、300×232，单色白底黑字、字体 0。path 仍为 A、B、C 三行，path_m=64；两份 val 分别为 0、1，存储为四字节。hig=72、dis=1、pco1=31、选中文字 pco2=63488、ch=8。

txt 为只读结果，保存的初始缓存为空；不手动修改。核对选择值、集合、预览和编译，并记录实际预览内容。up 为 sys2=select0.val。

## sltext（页面 ID 3）

slt0 位于 20/20、440×232，单色白底黑字、字体 0、左对齐；txt_maxl=64、path_m=0 自动、isbr=0、left=1、ch=8。

- XInteraction0.HMI 的 txt 为 A、B、C、D 四行。
- XInteraction1.HMI 的 txt 为 A、B、C、EE 四行。

完整内容以 txt 属性核对，静态预览允许只显示部分行；未要求实际滚动。原生 path 为空、maxval_y 初始 0 且只读、val_y 初始 0。up 为 slt0.val_y=0。

请反馈两份是否正常打开、各页属性／预览是否符合说明，以及编译结果和提示。源码在 project-0／project-1，每份实际执行 7 条独立命令，输出与统一入口相同，独立链接检查通过。
''', encoding="utf-8")
    print(f"X543 交互控件验收工程已生成：{output}")


if __name__ == "__main__":
    main()
