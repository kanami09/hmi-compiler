"""批量生成五类 X 系列控件的四种背景模式，共两份 HMI、20 页。"""
import argparse
from datetime import date
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
from prepare_pictures_acceptance import bitmap
from prepare_picture_modes_acceptance import panel
from prepare_files_acceptance import icon_png


CONTROLS = [("combo", "combo_box", "cb0"), ("select", "text_select", "ts0"),
            ("sltext", "sliding_text", "slt0"), ("browser", "file_browser", "fb0"),
            ("record", "data_record", "data0")]


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--font", type=Path, required=True, help="独立 UTF-8 ZI 字库，建议沿用已验收的字高68字库")
    parser.add_argument("--output", type=Path, default=root / "target/acceptance-x-backgrounds")
    parser.add_argument("--binaries", type=Path, default=root / "target/debug")
    args = parser.parse_args()
    output, binaries, font = args.output.resolve(), args.binaries.resolve(), args.font.resolve()
    if output.exists():
        parser.error("输出目录已存在，请使用 --output 指定新目录")
    if not font.is_file():
        parser.error("请提供独立 ZI 字库")
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
    for orientation, width, height, modes in [(0, 480, 272, [("pic", 2), ("solid", 1)]),
                                               (90, 272, 480, [("crop", 0), ("trans", 3)])]:
        name = f"XModes{orientation}"
        project = output / name
        run("hmi", "init", project, "--target", "TJC4827X543_011", "--orientation", orientation)
        assets = project / "assets"
        assets.mkdir()
        shutil.copyfile(font, assets / "ui.zi")
        (assets / "ui.asset.toml").write_text('schema="hmi.asset/1"\nkind="font"\nsource="ui.zi"\n', encoding="utf-8")
        inputs = {"full": ("bmp", bitmap(width, height)),
                  "combo_bg": ("bmp", panel(width - 40, 84, (190, 220, 250))),
                  "panel_bg": ("bmp", panel(width - 40, height - 40, (250, 195, 135))),
                  "folder": ("png", icon_png(True)), "file": ("png", icon_png(False))}
        for key, (extension, data) in inputs.items():
            (assets / f"{key}.{extension}").write_bytes(data)
            (assets / f"{key}.asset.toml").write_text(f'schema="hmi.asset/1"\nkind="image"\nsource="{key}.{extension}"\n', encoding="utf-8")
        keys = [*inputs, "ui"]
        if orientation == 90:
            keys.reverse()
        pages = [(f"{tag}_{short}", short, kind, control, mode)
                 for tag, mode in modes for short, kind, control in CONTROLS]
        sources = ",".join(f'"pages/{page}/page.toml"' for page, *_ in pages)
        items = ",".join('{key="' + key + '",source="assets/' + key + '.asset.toml"}' for key in keys)
        (project / "hmi.toml").write_text(f'''schema="hmi.project/1"
name="{name}"
[target]
profile="TJC4827X543_011"
format="ver21234"
encoding="utf-8"
orientation={orientation}
[program]
source="program.s"
[pages]
sources=[{sources}]
[assets]
items=[{items}]
''', encoding="utf-8")
        for index, (page, short, kind, control, mode) in enumerate(pages):
            directory = project / "pages" / page
            directory.mkdir()
            # 所有页面用网格作底图，便于比较透明背景与控件主动绘制的背景。
            props = ["x=20", "y=20", f"w={width - 40}", f'h={84 if short == "combo" else height - 40}',
                     f"sta={mode}", 'font={ref="ui"}', f'pic={{ref="{"combo_bg" if short == "combo" else "panel_bg"}"}}',
                     'picc={ref="full"}']
            if short in ["combo", "select"]:
                props += ['path="A\\nB\\nC"', "path_m=64", "val=1"]
                if short == "combo":
                    props += ['txt="INIT"']
            elif short == "sltext":
                props += ['txt="A\\nB\\nC\\nD"', "txt_maxl=64"]
            elif short == "browser":
                props += ['pic1={ref="folder"}', 'pic2={ref="file"}', 'dir="sd0/"', 'filter="*.txt|*.data"']
            else:
                # 每个控件声明独立运行时路径，不创建或读取实际 .data 文件。
                cell_width = (width - 60) // 2
                props += [f'path="sd0/xm{orientation}_{mode}.data"', "lenth=32", "maxval=16", "dez=2",
                          f'format="{cell_width}^{cell_width}"', 'dir="A^B"', "mode=1"]
            (directory / "page.toml").write_text(f'''schema="hmi.page/1"
name="{page}"
properties={{sta=2,pic={{ref="full"}}}}
[[objects]]
name="{control}"
type="{kind}"
properties={{{",".join(props)}}}
events={{down="down.s",up="up.s"}}
''', encoding="utf-8")
            (directory / "down.s").write_text(f"sys0={mode}\n", encoding="utf-8")
            (directory / "up.s").write_text(f"sys1={index + 1}\n", encoding="utf-8")
        run("hmi", "check", project, "-Werror")
        assert not (project / ".build").exists()
        hmi = output / f"{name}.HMI"
        run("hmi", "build", project, "-o", hmi, "-Werror")
        plan = project / ".build/manual"
        run("hmi", "plan", project, "-o", plan)
        jobs = json.loads((plan / "jobs.json").read_bytes())["jobs"]
        assert len(jobs) == 18
        for job in jobs:
            run(job["command"], *job["args"])
        assert hmi.read_bytes() == (project / f"dist/{name}/{name}.HMI").read_bytes()
        run("hmi-link", plan / "link.toml", "--check", "-Werror")
        for index, (_, short, _, _, mode) in enumerate(pages):
            inspected = run("hmi-compile", "inspect", plan / f"obj/p{index:05}.hmo")["objects"][1]["properties"]
            assert inspected["sta"] == mode
            assert inspected["pic"] == {"ref": "combo_bg" if short == "combo" else "panel_bg"}
            assert inspected["picc"] == {"ref": "full"}
            if short == "browser":
                assert inspected["pic1"] == {"ref": "folder"}
                assert inspected["pic2"] == {"ref": "file"}
        image_keys = [key for key in keys if key != "ui"]
        cases.append({"file": hmi.name, "sha256": hashlib.sha256(hmi.read_bytes()).hexdigest(),
                      "profile": "TJC4827X543_011", "orientation": orientation, "page_size": [width, height],
                      "pages": [{"name": p, "type": kind, "sta": mode} for p, _, kind, _, mode in pages],
                      "image_ids": {key: image_keys.index(key) for key in image_keys}, "font_id": 0,
                      "standalone_jobs": len(jobs), "standalone_equals_driver": True,
                      "opened": None, "compiled": None, "content_verified": None})
    (output / "verification.json").write_text(json.dumps({"generated_on": date.today().isoformat(),
        "stage": "five-x-controls-four-background-modes", "combinations": 20,
        "automatic_checks_passed": True, "cases": cases}, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    (output / "README.md").write_text('''# 五类 X 系列控件背景批量验收

本批只有两份 HMI，共20页，覆盖下拉框、选择文本、滑动文本、文件浏览器和数据记录的全部四种背景。请分别打开两份文件，核对各页属性和画面，最后每份编译一次即可。需要保存时请另存副本，原始摘要见 verification.json。

| 文件 | 型号／方向／尺寸 | 前5页 | 后5页 |
|---|---|---|---|
| XModes0.HMI | X543／0°／480×272 | pic_*：sta2图片 | solid_*：sta1单色，图片引用保留 |
| XModes90.HMI | X543／90°／272×480 | crop_*：sta0切图 | trans_*：sta3透明，图片引用保留 |

每组按 combo／select／sltext／browser／record 排列，每页一个普通控件ID1。页面自身ID0，均sta2、pic引用全屏四象限彩色网格。控件位置20/20，0°宽440、90°宽232；下拉框高84，其他控件高232／440。字体ID0，沿用独立字高68的ZI。

## 核对背景

- pic_*：下拉框为淡蓝棋盘，其余四类为淡橙棋盘背景。
- solid_*：恢复各自单色背景，保留的pic/picc不应使主体背景显示棋盘。
- crop_*：从全屏网格中取控件位置对应区域，不拉伸小背景图片。
- trans_*：主体背景透出页面网格。选中项、表头、下拉列表、边框等仍按原生颜色绘制，不要求它们也变透明。

四种模式都保留pic/picc，不能只按属性栏当前是否显示这些字段判断资源是否存在。无需切换模式查看隐藏字段。

## 核对内容

combo：cb0，选项A/B/C，val1，独立文本INIT，展开状态默认收缩。select：ts0，选项A/B/C，val1，运行时文本缓存为空，未预先替换成B。sltext：slt0，正文A/B/C/D，容量64。三者的行高、滑动参数和选中颜色保持已验收默认值。

browser：fb0，目录sd0/，过滤*.txt|*.data，pic1黄色文件夹、pic2蓝白文件图标，图标64×64；没有配置目录内容时可以为空列表。本批只检查背景、属性及编译，不要求创建测试目录或文件。

record：data0，lenth32/maxval16/dez2/mode1，表头A^B，0°列宽210^210、90°为106^106；每页独立路径sd0/xm<方向>_<sta>.data，不共用运行时文件。没有预置记录，qty0、val为原生未选中值。构建未生成或访问.data，本批无需准备虚拟SD，不要求运行时读写。

普通控件down事件sys0=sta，up事件sys1=页序号+1；页面事件为空。背景变化不应改写上述选项、文本、文件配置、图标或事件。

## 图片 ID

| 图片 | XModes0 | XModes90 |
|---|---|---|
| full全屏网格 | 0 | 4 |
| combo_bg淡蓝 | 1 | 3 |
| panel_bg淡橙 | 2 | 2 |
| folder文件夹 | 3 | 1 |
| file默认文件 | 4 | 0 |

下拉框pic引用combo_bg，其余四类pic引用panel_bg，所有picc及页面pic引用full；browser另pic1引用folder、pic2引用file。90°有意反转图片顺序，同时检查字体及背景方向。

资源全部内嵌，打开HMI无需字库源、FFmpeg或虚拟SD。每份实际执行18条独立作业，HMI与统一构建入口逐字节相同。源码、媒体描述、独立计划均在同名子目录；重新生成需要指定独立ZI，其余素材由代码生成。动态选择、滚动及文件读写不属于本批验收。
''', encoding="utf-8")
    print(output)


if __name__ == "__main__":
    main()
