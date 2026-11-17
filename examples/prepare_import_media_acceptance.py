"""导入已验收的五类媒体HMI，修改控件与资源顺序后生成两份最终工程。"""
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
import tomllib
from prepare_import_acceptance import page_text


def snapshot(path):
    data = path.read_bytes()
    resources = {}
    for index in range(struct.unpack_from("<I", data)[0]):
        start = 4 + index * 28
        name = data[start:start + 16].split(b"\0", 1)[0].decode("utf-8")
        offset, length = struct.unpack_from("<II", data, start + 16)
        if data[start + 24] == 0:
            resources[name] = (data[offset:offset + length], data[start + 25:start + 28])
    main = resources["main.HMI"][0]
    offset, count = struct.unpack_from("<II", main, 24)
    families = {}
    for index in range(count):
        kind = main[offset + index * 16:offset + index * 16 + 8].split(b"\0", 1)[0].decode("utf-8")
        name = main[offset + index * 16 + 8:offset + index * 16 + 16].split(b"\0", 1)[0].decode("utf-8")
        family = families.setdefault(kind, [])
        source_extension = {"i": "is", "gmov": "gmovs"}.get(kind)
        source = resources.get(name.rsplit(".", 1)[0] + "." + source_extension) if source_extension else None
        family.append((resources[name], source))
    return families, resources["Program.s"]


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--inputs", type=Path, nargs="+", required=True, help="上一批Inputs0.HMI、Inputs90.HMI（保留e/n/o媒体页结构）")
    parser.add_argument("--sd0", type=Path, help="配套文件的sd0源目录，省略取各输入HMI同目录的sd0")
    parser.add_argument("--output", type=Path, default=root / "target/acceptance-import-media")
    parser.add_argument("--binaries", type=Path, default=root / "target/debug")
    args = parser.parse_args()
    output, binaries = args.output.resolve(), args.binaries.resolve()
    if output.exists():
        parser.error("输出目录已存在，请使用 --output 指定新目录")
    inputs = [p.resolve() for p in args.inputs]
    if any(not p.is_file() for p in inputs):
        parser.error("请提供已验收的Inputs媒体HMI")
    suffix = ".exe" if os.name == "nt" else ""
    tools = {"hmi", "hmi-parse", "hmi-compile", "hmi-media", "hmi-link"}
    if any(not (binaries / (tool + suffix)).is_file() for tool in tools):
        parser.error("请先运行 cargo build --workspace --bins")
    environment = dict(os.environ, HMI_FFMPEG_DIR=str(output / "unavailable-backend"))

    def run(tool, *arguments):
        if tool not in tools:
            raise RuntimeError(f"执行计划包含未知工具：{tool}")
        result = subprocess.run([str(binaries / (tool + suffix)), *map(str, arguments), "--json"], env=environment,
                                capture_output=True, text=True, encoding="utf-8", check=False)
        if result.returncode:
            raise RuntimeError(result.stdout + result.stderr)
        data = json.loads(result.stdout)
        if not data["ok"] or data["diagnostics"]:
            raise RuntimeError(result.stdout)
        return data["result"]

    output.mkdir(parents=True)
    (output / "roundtrip").mkdir()
    cases, directions = [], set()
    for original in inputs:
        info = run("hmi-parse", "inspect", original)
        orientation = info["target"]["orientation"]
        if orientation in directions:
            raise RuntimeError("每个方向只接受一个验收输入")
        directions.add(orientation)
        print(f"正在导入 {original.name} 并核对{orientation}°媒体载荷", flush=True)
        project = output / f"imported-{orientation}"
        run("hmi", "import", original, "-o", project)
        assert not (project / ".build").exists()
        unchanged = output / f"roundtrip/MediaRound{orientation}.HMI"
        run("hmi", "check", project, "-Werror")
        run("hmi", "build", project, "-o", unchanged, "-Werror")
        assert snapshot(original) == snapshot(unchanged)
        manifest = project / "hmi.toml"
        source = tomllib.loads(manifest.read_text(encoding="utf-8"))
        name = f"MediaImport{orientation}"
        old_name = source["name"]
        text = manifest.read_text(encoding="utf-8").replace(f'name = "{old_name}"', f'name = "{name}"', 1)
        # 调换两个图片清单项，其key不变；控件的符号引用应跟随新的最终ID。
        images = [item for item in source["assets"]["items"] if item["key"].startswith("image_")]
        if len(images) < 2:
            raise RuntimeError("验收输入须含至少两张图片")
        a, b = images[:2]
        # TOML数组按完整条目重排，不替换资产key或源描述内容。
        lines = text.splitlines(keepends=True)
        header = "[[assets.items]]"
        blocks = []
        first = None
        for index, line in enumerate(lines):
            if line.strip() == header:
                if first is not None:
                    blocks.append((first, index))
                first = index
        if first is not None:
            blocks.append((first, len(lines)))
        if len(blocks) != len(source["assets"]["items"]):
            raise RuntimeError("当前验收器未识别资产数组排版")
        bodies = ["".join(lines[start:end]) for start, end in blocks]
        i = source["assets"]["items"].index(a)
        j = source["assets"]["items"].index(b)
        bodies[i], bodies[j] = bodies[j], bodies[i]
        manifest.write_text("".join(lines[:blocks[0][0]]) + "".join(bodies), encoding="utf-8")
        external = set()
        seen = set()
        for relative in source["pages"]["sources"]:
            path = project / relative
            page = tomllib.loads(path.read_text(encoding="utf-8"))
            seen.add(page["name"])
            for obj in page["objects"]:
                props = obj["properties"]
                if obj["type"] == "animation" and props.get("from") == 1:
                    runtime = props["path"]
                    if not isinstance(runtime, str) or not runtime.startswith("sd0/") or any(p in ["", ".", ".."] for p in runtime[4:].split("/")):
                        raise RuntimeError("验收外部路径须为安全的sd0相对路径")
                    external.add(runtime)
                if page["name"] == "n_image" and obj["type"] == "picture":
                    props["x"] = 70
                if page["name"] == "e_font" and obj["type"] == "text":
                    props["txt"] = "CAB"
                if page["name"] in ["n_anim", "o_video", "n_audio"] and obj["type"] in ["animation", "video", "audio"]:
                    props["loop"] = 1
                if page["name"] == "o_audio" and obj["type"] == "audio":
                    obj["name"] = "sound0"
            path.write_text(page_text(page), encoding="utf-8")
        if not {"n_image", "e_font", "n_anim", "o_video", "n_audio", "o_audio"}.issubset(seen):
            raise RuntimeError("验收器要求上一批Inputs工程的e/n/o页结构")
        for runtime in external:
            source_sd = args.sd0.resolve() if args.sd0 else original.parent / "sd0"
            src = source_sd / runtime[4:]
            if not src.is_file():
                raise RuntimeError(f"缺少配套运行GMOV：{src}")
            dest = output / runtime
            dest.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(src, dest)
        hmi = output / f"{name}.HMI"
        run("hmi", "build", project, "-o", hmi, "-Werror")
        plan = project / ".build/manual"
        run("hmi", "plan", project, "-o", plan)
        jobs = json.loads((plan / "jobs.json").read_bytes())["jobs"]
        actions = []
        for job in jobs:
            result = run(job["command"], *job["args"])
            if job["command"] == "hmi-media":
                actions.append(result["input_action"])
        assert len(actions) == len(source["assets"]["items"]) and set(actions) == {"native"}
        assert hmi.read_bytes() == (project / f"dist/{name}/{name}.HMI").read_bytes()
        cases.append({"file": hmi.name, "sha256": hashlib.sha256(hmi.read_bytes()).hexdigest(), "orientation": orientation,
                      "input_hmi": str(original), "input_sha256": hashlib.sha256(original.read_bytes()).hexdigest(),
                      "unchanged_logical_payloads_equal": True, "native_assets": len(actions), "standalone_jobs": len(jobs),
                      "without_backend": True, "standalone_equals_driver": True, "external_paths": sorted(external),
                      "opened": None, "compiled": None, "content_verified": None})
    (output / "verification.json").write_text(json.dumps({"generated_on": date.today().isoformat(), "stage": "import-native-media",
        "automatic_checks_passed": True, "cases": cases}, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    (output / "README.md").write_text('''# 五类媒体导入／修改／重建验收

最终只需验收根目录MediaImport0.HMI、MediaImport90.HMI：输入直接使用上一批已验收的Inputs0／Inputs90，每份15页。脚本通过正式导入命令生成新版源码，先核对未改重建的页面、逻辑媒体载荷、已有编辑源及目录未知字，再修改控件和资产顺序生成最终文件。roundtrip子目录只是未修改重建参考，不是额外必验文件。

请把本目录sd0中的input0／input90复制到上位机虚拟SD根目录；若上一批已复制且未删除，也可直接使用。两份最终HMI均X543，分别0°／480×272、90°／272×480。核对后各编译一次即可，本批不要求试听或实际播放。

页序仍为e/n/o三组，每组image/font/anim/video/audio。所有内嵌图片、字库、动画、视频、音频均导出为显式native HMA包；e_anim仍是from1的外部路径，导入器不去用户机器找SD文件。源码资产描述在imported-*的assets中，权威包在.preserve/assets；assets/view是可提取的标准编辑源或运行文件，仅用于查看，修改它们不会悄悄改变native输入。

本次指定修改：

- e_font的t0文字从ABC改CAB；其余两组字体文字仍ABC。
- n_image的p0横坐标从20改70，尺寸48×32及透明红蓝图不变。
- n_anim、o_video、n_audio的loop改1，en仍0，保持初始停止。
- o_audio控件从wav0重命名sound0，不追踪原名。
- 图片清单前两项交换，控件仍按符号引用：e_image的pic应从0变1，n_image从1变0，o_image仍2；三张图片内容相同，可直接核对字段及图片列表顺序。

其余属性和媒体内容保留。内部字体／视频／音频三组ID仍0/1/2，内嵌动画n/o为0/1，e_anim外部路径及vid65535保持。已有.is/.gmovs原样保留；原来缺.is的e_image仍缺，不补造原图。原始质量或转换请求无法可靠恢复时不猜测参数，也不把当前编码后端当作原厂后端。

全部导入、检查、构建及独立作业在无可用FFmpeg环境中完成；仅复用原生资源，无需本工具DLL、独立字库或原始AVI/GIF。每份15页、14个内部资产包、31条独立作业，HMI与统一入口逐字节一致。源HMI只在导入时读取，后续构建只使用工程显式文件和原始附件。
''', encoding="utf-8")
    print(output)


if __name__ == "__main__":
    main()
