# FFmpeg 后端配置

[项目首页](../../../README.md) · [媒体编码器](../README.md) · [输入方式与部署](inputs.md)

## 使用条件

视频源解码和需要转换的音频输入使用 FFmpeg shared DLL，当前仅支持 Windows x64。普通图片、GIF、ZI、原参数 PCM16 WAV，以及已有原生包和独立链接不需要该后端。

工具不调用 `ffmpeg.exe`，也不会自动下载 DLL。不同架构或不兼容库版本不能混用。

## 库目录

```powershell
$env:HMI_FFMPEG_DIR = "D:\tools\ffmpeg\bin"
hmi-media backend --json
```

未设置环境变量时，读取可执行程序旁的 `ffmpeg` 子目录：

```text
bin/
├── hmi.exe
├── hmi-media.exe
└── ffmpeg/
    ├── avutil-61.dll
    ├── swresample-7.dll
    ├── avcodec-63.dll
    ├── avformat-63.dll
    └── swscale-10.dll
```

这五个库应来自同一套兼容的 shared 构建，并保留该构建要求的其他依赖 DLL。配置目录不会修改系统 PATH 或 HMI 上位机目录。

## 兼容版本

当前后端要求以下 ABI 主版本，且库版本不早于所列基线：

| 库 | 最低版本 |
|---|---|
| avutil | 61.1.102 |
| swresample | 7.1.102 |
| avcodec | 63.1.102 |
| avformat | 63.1.102 |
| swscale | 10.1.102 |

已使用 FFmpeg `n9.0.2-49-gc179411bbb-20261111` shared 构建验证。仅以“FFmpeg 9”或 ZIP 文件名相同不足以确认兼容，应运行 `hmi-media backend --json` 检查目录、实际版本和指纹。

该构建来自 [FFmpeg-Builds](https://github.com/FFmpeg-Builds/FFmpeg-Builds)，验证时的包名为 `ffmpeg-n9.0-latest-win64-lgpl-shared-9.0.zip`。其 SHA-256 为：

```text
17059ded875218b0251977abab6aea21adb6937bb4fd2130c81ada5c89f879d4
```

`latest` 下载内容可能更新，上述摘要用于识别验证过的包，不能根据相同包名假定文件未变。使用其他满足兼容条件的构建时，同样先检查后端。

`backend` 失败时先检查进程架构、DLL 是否齐全、是否来自同一构建以及版本是否满足要求。每个进程使用一组库；切换目录后重新启动命令。

视频输入及转换限制见[视频参考](video.md)，音频参数见[音频参考](audio.md)。
