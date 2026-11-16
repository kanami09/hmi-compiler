"""生成 X543 外部透明图片验收工程及部署文件，不访问虚拟 SD。"""
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
from prepare_alpha_acceptance import ring_png
from prepare_pictures_acceptance import bitmap


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=root / "target/acceptance-external")
    parser.add_argument("--binaries", type=Path, default=root / "target/debug")
    args = parser.parse_args()
    output, binaries = args.output.resolve(), args.binaries.resolve()
    if output.exists():
        parser.error("输出目录已存在，请使用 --output 指定新目录")
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
    for orientation, width, height in [(0, 480, 272), (90, 272, 480)]:
        project = output / f"project-{orientation}"
        run("hmi", "init", project, "--target", "TJC4827X543_011", "--orientation", orientation)
        assets = project / "assets"
        assets.mkdir()
        (assets / "background.bmp").write_bytes(bitmap(width, height))
        (assets / "ring.png").write_bytes(ring_png())
        (assets / "background.asset.toml").write_text('schema="hmi.asset/1"\nkind="image"\nsource="background.bmp"\n', encoding="utf-8")
        runtime_path = f"sd0/hmic_external{orientation}.xi"
        (assets / "ring.asset.toml").write_text(f'''schema="hmi.asset/1"
kind="image"
source="ring.png"
[encoding]
quality=96
alpha="preserve"
[resize]
width=138
height=106
mode="stretch"
[delivery]
mode="external"
path="{runtime_path}"
''', encoding="utf-8")
        keys = ["ring", "background"] if orientation == 0 else ["background", "ring"]
        items = ",".join('{key="' + k + '",source="assets/' + k + '.asset.toml"}' for k in keys)
        (project / "hmi.toml").write_text(f'''schema="hmi.project/1"
name="External{orientation}"
[target]
profile="TJC4827X543_011"
format="ver21234"
encoding="utf-8"
orientation={orientation}
[program]
source="program.s"
[pages]
sources=["pages/page0/page.toml"]
[assets]
items=[{items}]
''', encoding="utf-8")
        positions = [(24, 18), (276, 142)] if orientation == 0 else [(16, 32), (112, 200)]
        text = f'''schema="hmi.page/1"
name="external"
[[objects]]
name="bg0"
type="picture"
properties={{x=0,y=0,w={width},h={height},pic={{ref="background"}}}}
'''
        for index, (x, y) in enumerate(positions):
            text += f'''[[objects]]
name="exp{index}"
type="external_picture"
properties={{x={x},y={y},w=138,h=106,path={{ref="ring"}}}}
'''
        (project / "pages/page0/page.toml").write_text(text, encoding="utf-8")
        run("hmi", "check", project, "-Werror")
        assert not (project / ".build").exists()
        hmi = output / f"External{orientation}.HMI"
        result = run("hmi", "build", project, "-o", hmi, "-Werror")
        assert result["external_files"] == [runtime_path]
        xi = output / runtime_path
        data = xi.read_bytes()
        assert data[:8] == b"hmipicxi" and data[8] == 10 + orientation // 90
        assert struct.unpack_from("<IHHI", data, 16) == (24, 138, 106, len(data) - 32)
        assert 0 < struct.unpack_from("<I", data, 28)[0] < len(data) - 32
        map_path = output / f"External{orientation}.link-map.json"
        shutil.copyfile(output / "link-map.json", map_path)
        driver_map = json.loads(map_path.read_bytes())
        assert len(driver_map["resources"]) == 5
        assert driver_map["runtime_files"][0]["generated"] is True
        assert len(driver_map["runtime_files"][0]["users"]) == 2
        plan = project / ".build/manual"
        run("hmi", "plan", project, "-o", plan)
        jobs = json.loads((plan / "jobs.json").read_bytes())["jobs"]
        manual_map = project / "standalone-map.json"
        for job in jobs:
            extra = ["--map", manual_map, "-Werror"] if job["command"] == "hmi-link" else []
            run(job["command"], *job["args"], *extra)
        manual = project / f"dist/External{orientation}"
        assert hmi.read_bytes() == (manual / hmi.name).read_bytes()
        assert data == (manual / runtime_path).read_bytes()
        assert driver_map == json.loads(manual_map.read_bytes())
        run("hmi-link", plan / "link.toml", "--check", "-Werror")
        inspection = run("hmi-compile", "inspect", plan / "obj/p00000.hmo")
        assert inspection["objects"][2]["properties"]["path"] == {"ref": "ring"}
        cases.append({"file": hmi.name, "sha256": hashlib.sha256(hmi.read_bytes()).hexdigest(),
                      "profile": "TJC4827X543_011", "orientation": orientation, "page_size": [width, height],
                      "external_file": runtime_path, "external_sha256": hashlib.sha256(data).hexdigest(),
                      "logical_image_size": [138, 106], "positions": positions,
                      "standalone_jobs": len(jobs), "standalone_equals_driver": True,
                      "opened": None, "compiled": None, "content_verified": None})
    (output / "verification.json").write_text(json.dumps({"generated_on": date.today().isoformat(),
        "stage": "external-picture-xi", "automatic_checks_passed": True, "cases": cases},
        ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    (output / "README.md").write_text('''# 外部图片验收

先将本目录 sd0/ 下的 hmic_external0.xi、hmic_external90.xi 复制到上位机的虚拟 SD 根目录，再打开 External0.HMI、External90.HMI。这两个文件是本工具生成的正式配套文件，不能只复制 HMI 后检查预览。工具未自动访问或修改虚拟 SD。

若目标位置已有同名文件，请先确认来源并保留已有数据；如需更换实验路径，可修改源码资产描述中的 delivery.path 后构建，无需手动逐个改控件。无需使用官方转换工具重新转换 XI。

两份均为 TJC4827X543_011，一页 external，四个对象：页面自身 ID 0、背景 bg0 ID 1、外部图片 exp0／exp1 ID 2／3。图片列表只有背景，ID 始终为 0；外部图片不占普通图片 ID。背景为彩色分区和网格，两个青色圆环共用一份外部透明图片。

| 工程 | 页面尺寸 | 外部文件路径 | exp0 的 x/y | exp1 的 x/y |
|---|---|---|---|---|
| External0.HMI | 480×272，0° | sd0/hmic_external0.xi | 24/18 | 276/142 |
| External90.HMI | 272×480，90° | sd0/hmic_external90.xi | 16/32 | 112/200 |

两个外部控件均为 138×106、私有，无事件。源 PNG 为 69×53，通过资产显式 resize 放大两倍，JPEG 质量 96，保留透明度。XI 的逻辑尺寸仍为 138×106，编码内部有必要的补齐和旋转；不要因编码光栅尺寸与控件不同而修改属性。

请检查：两份方向与尺寸正确；外部控件路径正确，两个圆环均可显示；中央孔与四角透明，边缘有半透明过渡，能看到下方网格和底色；普通图片列表中只有背景；分别编译通过。缺图时先检查配套文件是否复制到正确的虚拟 SD 路径，不必将 XI 导入普通图片列表来修复。

请反馈打开、属性／显示及编译结果和具体提示。需要保存时请另存副本，原始摘要见 verification.json。本批不要求运行模拟器、读写数据记录或连接硬件。

project-0／project-90 为可编辑源码。每份实际执行五条独立作业，HMI、外部 XI 和完整链接映射与统一入口一致。各自的 External0.link-map.json／External90.link-map.json 列出配套路径、摘要及使用者。本脚本只使用代码生成的图片，不读取已有 HMI、内部素材或字库。
''', encoding="utf-8")
    print(f"外部图片验收工程已生成：{output}")


if __name__ == "__main__":
    main()
