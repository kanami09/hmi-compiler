# 媒体编码器

[项目首页](../../README.md) · [输入与部署](reference/inputs.md) · [FFmpeg 配置](reference/ffmpeg.md) · [通用输出选项](../hmi-cli/reference/automation.md)

`hmi-media` 探测和处理媒体，生成自包含 `.hma` 资产目标供链接器使用。源文件保持不变，实际处理方式由资产描述明确指定。

## 命令

```text
hmi-media probe SOURCE --target TARGET
hmi-media encode SOURCE --target TARGET -o OUTPUT
hmi-media inspect FILE
hmi-media backend
hmi-media builtin ID --target TARGET -o OUTPUT
```

| 命令 | 作用 |
|---|---|
| `probe` | 检查资产输入及处理参数，不写文件；源输入可能完整解码 |
| `encode` | 按输入方式编码、封装或保留媒体，输出 `.hma` |
| `inspect` | 校验并查看已有 `.hma` 描述及成员 |
| `backend` | 检查 FFmpeg DLL 目录、版本和兼容性 |
| `builtin` | 生成受支持的内置图标目标 |

`SOURCE` 为资产 TOML 描述，`TARGET` 为当前工具生成的 `target.json`。`-o/--output` 必须指向与输入不同的位置。

```powershell
hmi-media probe assets/logo.asset.toml --target build/target.json
hmi-media encode assets/logo.asset.toml --target build/target.json -o build/logo.hma
hmi-media inspect build/logo.hma --json
```

## 媒体类型

| 类型 | 源输入与输出 | 详细参考 |
|---|---|---|
| 图片 | BMP/PNG/JPG；T0/K0 的 RGB565、X543 的 JPEG，X543 可保留透明度 | [图片](reference/images.md) |
| 字库 | 受支持的 UTF-8 ZI v6，按目标自动适配方向 | [字库](reference/fonts.md) |
| 动画 | GIF，生成内嵌动画或外部 GMOV | [动画](reference/animation.md) |
| 视频 | 受支持的固定帧率视频，生成有声或无声 VIDEO | [视频](reference/video.md) |
| 音频 | WAV，生成 PCM16 运行资源，可转换或截取 | [音频](reference/audio.md) |

已编码运行文件、原生资产包和条件复用也由同一工具处理，见[输入方式](reference/inputs.md#输入方式)。不能通过改文件后缀获得其他编码格式。

## 资产描述

```toml
schema = "hmi.asset/1"
kind = "image"
source = "images/logo.png"

[encoding]
alpha = "flatten"
background = "#FFFFFF"
```

描述中的路径相对于描述文件。工程资产表为描述指定 `key`，控件按当前符号引用。`input`、`original` 和 `delivery` 的区别及外部文件部署，集中见[通用参考](reference/inputs.md)。

## 后端与进度

图片、GIF、字库和原参数 PCM16 WAV 不需要 FFmpeg。视频及需要采样转换的音频使用 Windows x64 的兼容 FFmpeg shared 库；配置方法见[后端说明](reference/ffmpeg.md)。已有资产包可以在无 FFmpeg 的环境中链接。

处理计数来自实际解码、编码或音频转换。未知总量只显示已完成数量和用时；保留包及缓存复用不伪称重新解码。独立命令也支持 JSON 和 JSONL。

## 内置图标

```powershell
hmi-media builtin browser-folder --target build/target.json -o build/folder.hma
hmi-media builtin browser-file --target build/target.json -o build/file.hma
```

用于自行执行含默认文件浏览器图标的工具计划。统一入口会自动补齐所需资源，多页共享。图标配置见[文件控件参考](../hmi-compiler/reference/controls-files.md)。

## Rust 库

Cargo 包和库分别为 `hmi-media`、`hmi_media`。共享库与命令行使用相同输入处理规则，可通过可选回调收集进度。

```powershell
cargo doc -p hmi-media --no-deps --open
```
