"""生成页面背景和 X 系列文字控件透明背景的验收工程。"""
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


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--font", type=Path, required=True, help="独立 UTF-8 ZI 字库，建议字高 68")
    parser.add_argument("--output", type=Path, default=root / "target/acceptance-page-backgrounds")
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
    controls = [("text", "text", "t0"), ("button", "button", "b0"),
                ("number", "number", "n0"), ("xfloat", "xfloat", "x0"),
                ("dual", "dual_state_button", "bt0"), ("scroll", "scrolling_text", "g0")]
    specs = [("PageBgK", "TJC4832K035_011", 0, 320, 480),
             ("Transparent0", "TJC4827X543_011", 0, 480, 272),
             ("Transparent90", "TJC4827X543_011", 90, 272, 480)]
    for name, profile, orientation, width, height in specs:
        transparent = name.startswith("Transparent")
        project = output / name
        run("hmi", "init", project, "--target", profile, "--orientation", orientation)
        assets = project / "assets"
        assets.mkdir()
        inputs = {"background": bitmap(width, height)}
        if transparent:
            inputs.update({"panel": panel(width - 40, 100, (190, 220, 250)),
                           "pressed": panel(width - 40, 100, (250, 195, 135))})
            shutil.copyfile(font, assets / "ui.zi")
            (assets / "ui.asset.toml").write_text('schema="hmi.asset/1"\nkind="font"\nsource="ui.zi"\n', encoding="utf-8")
        for key, data in inputs.items():
            (assets / f"{key}.bmp").write_bytes(data)
            (assets / f"{key}.asset.toml").write_text(f'schema="hmi.asset/1"\nkind="image"\nsource="{key}.bmp"\n', encoding="utf-8")
        keys = [*inputs, "ui"] if transparent else [*inputs]
        if orientation == 90:
            keys.reverse()
        pages = controls if transparent else [("image", "", ""), ("solid", "", ""), ("none", "", "")]
        sources = ",".join('"pages/' + page + '/page.toml"' for page, _, _ in pages)
        items = ",".join('{key="' + key + '",source="assets/' + key + '.asset.toml"}' for key in keys)
        (project / "hmi.toml").write_text(f'''schema="hmi.project/1"
name="{name}"
[target]
profile="{profile}"
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
        for index, (page, kind, control) in enumerate(pages):
            directory = project / "pages" / page
            directory.mkdir()
            # 单色和无背景仍保留 pic，验证非活动资源不会改变当前显示模式。
            mode = 2 if transparent else [2, 1, 0][index]
            text = f'schema="hmi.page/1"\nname="{page}"\nproperties={{sta={mode},bco=2016,pic={{ref="background"}}}}\n'
            if transparent:
                props = ["x=20", "y=40", f"w={width - 40}", "h=100", "sta=3", 'font={ref="ui"}',
                         'pic={ref="panel"}', 'picc={ref="background"}']
                if kind in ["number", "xfloat"]:
                    props.append("val=1234")
                else:
                    props.append('txt="OK"' if page in ["button", "dual"] else 'txt="ABC"')
                if page in ["button", "dual"]:
                    props += ['pic2={ref="pressed"}', 'picc2={ref="background"}']
                if page == "dual":
                    props.append(f"val={int(orientation == 90)}")
                text += f'[[objects]]\nname="{control}"\ntype="{kind}"\nproperties={{{",".join(props)}}}\n'
                text += 'events={down="down.s",up="up.s"}\n'
                (directory / "down.s").write_text("sys0=1\n", encoding="utf-8")
                (directory / "up.s").write_text("sys0=2\n", encoding="utf-8")
            (directory / "page.toml").write_text(text, encoding="utf-8")
        run("hmi", "check", project, "-Werror")
        assert not (project / ".build").exists()
        hmi = output / f"{name}.HMI"
        run("hmi", "build", project, "-o", hmi, "-Werror")
        plan = project / ".build/manual"
        run("hmi", "plan", project, "-o", plan)
        jobs = json.loads((plan / "jobs.json").read_bytes())["jobs"]
        for job in jobs:
            run(job["command"], *job["args"])
        assert hmi.read_bytes() == (project / f"dist/{name}/{name}.HMI").read_bytes()
        run("hmi-link", plan / "link.toml", "--check", "-Werror")
        for index in range(len(pages)):
            objects = run("hmi-compile", "inspect", plan / f"obj/p{index:05}.hmo")["objects"]
            assert objects[0]["properties"]["pic"] == {"ref": "background"}
            assert objects[0]["properties"]["sta"] == (2 if transparent else [2, 1, 0][index])
            if transparent:
                assert objects[1]["properties"]["sta"] == 3
                assert objects[1]["properties"]["pic"] == {"ref": "panel"}
        image_keys = [key for key in keys if key != "ui"]
        cases.append({"file": hmi.name, "sha256": hashlib.sha256(hmi.read_bytes()).hexdigest(),
                      "profile": profile, "orientation": orientation, "page_size": [width, height],
                      "pages": [p[0] for p in pages], "image_ids": {key: image_keys.index(key) for key in image_keys},
                      "font_id": 0 if transparent else None, "standalone_jobs": len(jobs),
                      "standalone_equals_driver": True, "opened": None, "compiled": None, "content_verified": None})
    (output / "verification.json").write_text(json.dumps({"generated_on": date.today().isoformat(),
        "stage": "page-and-transparent-backgrounds", "automatic_checks_passed": True,
        "cases": cases}, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    (output / "README.md").write_text('''# 页面与透明背景验收

请打开以下三份 HMI，核对页面／控件属性和画面，并在上位机编译。资源均已内嵌，无需 FFmpeg 或虚拟 SD。需要保存时请另存副本，原始摘要见 verification.json。

| 文件 | 型号／方向／页面尺寸 | 内容 |
|---|---|---|
| PageBgK.HMI | K035／0°／320×480 | image、solid、none 三页，无普通控件 |
| Transparent0.HMI | X543／0°／480×272 | text、button、number、xfloat、dual、scroll 六页 |
| Transparent90.HMI | X543／90°／272×480 | 同上六类，图片 ID 顺序反转 |

PageBgK：页面自身 ID0，三页 sta 分别为2图片／1单色／0无背景，bco 均2016（绿色），pic 均0。image 显示全屏四象限彩色网格，solid 显示绿色，不受保留的 pic 影响；none 不主动绘制背景，核对 sta=0 及编译即可，不指定其预览颜色。页面几何取整个画布。

Transparent 两份：每页页面自身 sta=2，pic 引用全屏彩色网格；每页一个普通控件 ID1，位置20/40、高100、宽440或232、sta=3。文字或数字背景应透出页面网格，不能出现保留的淡蓝／淡橙图片填充。原生按钮边框或其他风格保持默认；透明不等于取消边框。text/scroll 文本ABC，button/dual文本OK，number为1234，xfloat为12.34。字体ID0，建议字高68；dual初值在0°为0、90°为1。scroll默认dir1/dis4/tim80/en1。普通控件down/up事件为sys0=1/2，页面事件为空。

Transparent0图片background/panel/pressed为ID0/1/2，Transparent90为ID2/1/0。页面pic分别0/2；六类pic均1，picc均全屏ID；button/dual另保留pic2为pressed、picc2为全屏ID。背景、图片绑定、字体方向均应正确。非活动配置直接核对原生属性即可，无需切换模式。

独立工具实际执行作业：K035为6条，两份X543各12条；输出均与统一构建入口逐字节相同。源码、媒体描述及独立计划位于各同名子目录；重新生成需要独立ZI，验收HMI无需字库源文件。本批不要求模拟器、按压、滚动或硬件测试。
''', encoding="utf-8")
    print(output)


if __name__ == "__main__":
    main()
