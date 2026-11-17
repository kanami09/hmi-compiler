"""生成无媒体已有工程导入／修改／重建验收，两份最终HMI、各五页。"""
import argparse
from datetime import date
import hashlib
import json
import os
from pathlib import Path
import struct
import subprocess
import sys
import tomllib


def value(item):
    if isinstance(item, bool):
        return str(item).lower()
    if isinstance(item, str):
        return json.dumps(item, ensure_ascii=False)
    if isinstance(item, int):
        return str(item)
    if isinstance(item, list):
        return "[" + ",".join(value(v) for v in item) + "]"
    if isinstance(item, dict):
        return "{" + ",".join(f"{k}={value(v)}" for k, v in item.items()) + "}"
    raise ValueError(item)


def page_text(page):
    text = "".join(f"{k}={value(page[k])}\n" for k in ["schema", "name", "original_record", "properties", "events"] if k in page)
    for obj in page.get("objects", []):
        text += "[[objects]]\n" + "".join(f"{k}={value(v)}\n" for k, v in obj.items())
    return text


def resources(path):
    data = path.read_bytes()
    result = {}
    for n in range(struct.unpack_from("<I", data)[0]):
        start = 4 + n * 28
        name = data[start:start + 16].split(b"\0", 1)[0].decode("utf-8")
        offset, length = struct.unpack_from("<II", data, start + 16)
        if data[start + 24] == 0:
            result[name] = data[offset:offset + length]
    return result


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=root / "target/acceptance-import")
    parser.add_argument("--binaries", type=Path, default=root / "target/debug")
    args = parser.parse_args()
    output, binaries = args.output.resolve(), args.binaries.resolve()
    if output.exists():
        parser.error("输出目录已存在，请使用 --output 指定新目录")
    suffix = ".exe" if os.name == "nt" else ""
    tools = {"hmi", "hmi-parse", "hmi-compile", "hmi-link"}
    if any(not (binaries / (tool + suffix)).is_file() for tool in tools):
        parser.error("请先运行 cargo build --workspace --bins")

    def run(tool, *arguments):
        if tool not in tools:
            raise RuntimeError(f"执行计划包含未知工具：{tool}")
        result = subprocess.run([str(binaries / (tool + suffix)), *map(str, arguments), "--json"], capture_output=True, text=True, encoding="utf-8", check=False)
        if result.returncode:
            raise RuntimeError(result.stdout + result.stderr)
        data = json.loads(result.stdout)
        if not data["ok"] or data["diagnostics"]:
            raise RuntimeError(result.stdout)
        return data["result"]

    output.mkdir(parents=True)
    (output / "sources").mkdir()
    (output / "roundtrip").mkdir()
    cases, names = [], ["gauge", "wave", "qrcode", "select", "logic"]
    for orientation, width, height in [(0, 480, 272), (90, 272, 480)]:
        print(f"正在创建、导入和修改 {orientation}° 无媒体工程", flush=True)
        seed = output / f"seed-project-{orientation}"
        run("hmi", "init", seed, "--target", "TJC4827X543_011", "--orientation", orientation)
        sources = ",".join(f'"pages/{name}/page.toml"' for name in names)
        (seed / "hmi.toml").write_text(f'''schema="hmi.project/1"
name="Seed{orientation}"
[target]
profile="TJC4827X543_011"
format="ver21234"
encoding="utf-8"
orientation={orientation}
[program]
source="program.s"
[pages]
sources=[{sources}]
''', encoding="utf-8")
        for index, name in enumerate(names):
            directory = seed / "pages" / name
            directory.mkdir()
            objects = []
            def add(control, kind, **props):
                objects.append({"name": control, "type": kind, "properties": props})
            if name == "gauge":
                add("z0", "gauge", x=20, y=20, w=100, h=100, val=45)
                add("z1", "gauge", x=140, y=20, w=100, h=100, val=135)
            elif name == "wave":
                add("s0", "waveform", x=20, y=20, w=232, h=80, ch=1)
                add("s1", "waveform", x=20, y=120, w=232, h=80, ch=2, dir=1)
                for n, obj in enumerate(objects):
                    obj["events"] = {"up": f"wave{n}.s"}
                    (directory / f"wave{n}.s").write_text(f"sys1={n + 1}\n", encoding="utf-8")
            elif name == "qrcode":
                add("qr0", "qrcode", x=20, y=20, w=160, h=160, txt="HMI-A", txt_maxl=16)
            elif name == "select":
                add("c0", "checkbox", x=20, y=20, w=64, h=64, val=0)
                add("r0", "radio", x=120, y=20, w=64, h=64, val=0)
                add("j0", "progress_bar", x=20, y=130, w=232, h=40, val=40)
            else:
                add("n0", "variable", val=111)
                add("tm0", "timer", tim=800, en=0)
                add("m0", "hotspot", x=20, y=20, w=100, h=80)
                add("tc0", "touch_cap")
                add("fs0", "file_stream")
            for control, x, destination, color in [("back", 12, names[(index - 1) % len(names)], 31), ("forward", width - 60, names[(index + 1) % len(names)], 2016)]:
                add(control, "progress_bar", x=x, y=height - 36, w=48, h=24, val=100, bco=color, pco=color)
                objects[-1]["events"] = {"up": f"{control}.s"}
                (directory / f"{control}.s").write_text(f"page {destination}\n", encoding="utf-8")
            (directory / "page.toml").write_text(page_text({"schema": "hmi.page/1", "name": name, "objects": objects}), encoding="utf-8")
        original = output / f"sources/Seed{orientation}.HMI"
        run("hmi", "build", seed, "-o", original, "-Werror")
        run("hmi-parse", "inspect", original)
        imported = output / f"imported-{orientation}"
        run("hmi" if orientation == 0 else "hmi-parse", "import" if orientation == 0 else "export", original, "-o", imported)
        assert not (imported / ".build").exists()
        unchanged = output / f"roundtrip/Round{orientation}.HMI"
        run("hmi", "check", imported, "-Werror")
        run("hmi", "build", imported, "-o", unchanged, "-Werror")
        assert resources(original) == resources(unchanged)
        project = tomllib.loads((imported / "hmi.toml").read_text(encoding="utf-8"))
        for source in project["pages"]["sources"]:
            path = imported / source
            page = tomllib.loads(path.read_text(encoding="utf-8"))
            if page["name"] == "gauge":
                page["objects"][0]["properties"]["val"] = 135
                page["objects"][1]["properties"]["down"] = -10
            elif page["name"] == "wave":
                page["objects"][0], page["objects"][1] = page["objects"][1], page["objects"][0]
            elif page["name"] == "qrcode":
                page["objects"][0]["name"] = "code0"
                page["objects"][0]["properties"]["txt"] = "HMI-IMPORT"
            elif page["name"] == "select":
                for obj, val in zip(page["objects"][:3], [1, 1, 75]):
                    obj["properties"]["val"] = val
            elif page["name"] == "logic":
                page["objects"][0]["name"] = "counter"
                del page["objects"][0]["properties"]["val"]
            path.write_text(page_text(page), encoding="utf-8")
        name = f"Import{orientation}"
        manifest = imported / "hmi.toml"
        manifest.write_text(manifest.read_text(encoding="utf-8").replace(f'name = "Seed{orientation}"', f'name = "{name}"', 1), encoding="utf-8")
        hmi = output / f"{name}.HMI"
        run("hmi", "build", imported, "-o", hmi, "-Werror")
        plan = imported / ".build/manual"
        run("hmi", "plan", imported, "-o", plan)
        assert (plan / "original.hpo").is_file()
        jobs = json.loads((plan / "jobs.json").read_bytes())["jobs"]
        assert len(jobs) == 7 and all(job["command"] != "hmi-media" for job in jobs)
        for job in jobs:
            run(job["command"], *job["args"])
        assert hmi.read_bytes() == (imported / f"dist/{name}/{name}.HMI").read_bytes()
        cases.append({"file": hmi.name, "sha256": hashlib.sha256(hmi.read_bytes()).hexdigest(), "orientation": orientation,
                      "pages": names, "source_hmi": str(original.relative_to(output)), "unchanged_active_resources_equal": True,
                      "standalone_jobs": len(jobs), "standalone_equals_driver": True, "opened": None, "compiled": None, "content_verified": None})
    (output / "verification.json").write_text(json.dumps({"generated_on": date.today().isoformat(), "stage": "import-no-embedded-media",
        "automatic_checks_passed": True, "cases": cases}, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    (output / "README.md").write_text('''# 已有工程导入／修改／重建验收

最终只需验收根目录Import0.HMI、Import90.HMI：均为X543，分别0°／480×272和90°／272×480，各五页。每份先创建无媒体HMI，再正式导入新版源码，未修改重建的全部活动资源与输入一致，之后编辑源码生成最终HMI。无需字库、FFmpeg或虚拟SD。需要保存时请另存副本，原始摘要见verification.json。

各页底部左蓝色／右绿色条块可循环切页，顺序gauge→wave→qrcode→select→logic。它们是带up事件的进度条，不依赖图片或字体。请核对各页并各编译一次，导航可在模拟器辅助查看，不要求硬件测试。

| 页面 | 最终内容 |
|---|---|
| gauge | z0位置20/20、100×100，val由45改135；z1位置140/20、100×100，val135，尾长down改-10 |
| wave | 原先s0/s1重排为s1 ID1、s0 ID2；位置仍分别20/120及20/20、232×80，ch2／1。up事件仍分别sys1=2／1，不按旧ID改绑 |
| qrcode | qr0重命名为code0，位置20/20、160×160，文本改HMI-IMPORT，容量16，无中心LOGO |
| select | c0/r0从0改1，位置20/20及120/20、64×64；j0在20/130、232×40，val从40改75 |
| logic | n0改名counter，删除已导出的val后使用创建默认0，不从原始附件取回111；tm0 tim800/en0，m0热区20/20/100×80，tc0触摸捕捉及fs0私有文件流保持原配置 |

source/Seed*.HMI是输入参考，roundtrip/Round*.HMI是未修改重建参考，不是额外必验文件。imported-*是最终源码工程，.preserve/origin.hpo保存活动原始记录及容器信息；页面和控件的original_record只定位未知数据，当前名称、已知属性和ID仍由源码及数组顺序决定。路径采用编号目录，不由路径推断当前页面ID。

每份最终实际执行7条独立作业，与统一构建入口的HMI逐字节一致；计划中显式携带original.hpo，页面编译也显式提供附件。链接器只用自包含目标及声明的附件，不回读原HMI或页面源码。本批尚未接入嵌入图片、字体、动画、音视频资产导入；遇到这些有效资源会明确拒绝，不丢弃后伪装成功。
''', encoding="utf-8")
    print(output)


if __name__ == "__main__":
    main()
