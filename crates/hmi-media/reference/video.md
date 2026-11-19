# 视频资产

[项目首页](../../../README.md) · [媒体编码器](../README.md) · [输入方式与部署](inputs.md)

源视频通过 [Windows x64 FFmpeg 后端](ffmpeg.md)解码，输出 HMI 支持的 VIDEO。需先配置后端，再执行 `probe/encode`。

```toml
schema = "hmi.asset/1"
kind = "video"
source = "intro.mp4"

[encoding]
quality = 80
fps = 4

[clip]
start_ms = 0
duration_ms = 1000

[audio]
mode = "drop"
```

| 字段 | 当前行为 |
|---|---|
| source | 本地原始视频，相对于资产描述文件；实际编码由 FFmpeg 判断，不能仅改后缀冒充。保留源画面尺寸，当前没有缩放参数 |
| encoding.quality | JPEG 质量，1～100、默认 80；100 仍是 Baseline YUV420 JPEG |
| encoding.fps | 必填，4、6、8、10、12、14、18、24 中的一项，不高于源帧率；要求源帧率÷目标帧率为正整数，例如 24→4／6／8／12／24 |
| clip.start_ms | 默认 0，以第一幅有效解码画面为起点；须精确落在源帧边界 |
| clip.duration_ms | 正整数，省略时使用 start 之后的有效画面长度；结束位置须落在源帧边界，时长须包含整数个目标帧。超出范围、非整帧或不能精确表示的边界报错，不补帧、不静默取整 |
| audio.mode | 默认 if-present，有一条音轨时保留、无音轨时无声；drop 显式丢弃；require 必须有音轨。需要保留多音轨输入时拒绝，不擅自选轨或混音 |
| audio.sample_rate／channels | 输出音轨默认 8000Hz／单声道；可选采样率 8000、11025、22050、24000、32000、44100，声道 1／2；转换为 s16 PCM。drop 时禁止填写，if-present 无音轨时报告 audio_applied=false |
| delivery | 默认 embedded；external 输出 sd0/ 下的 .video，不占内部视频 ID |

## 时序和音频

只支持时间戳精确等间隔的固定帧率。截取边界必须符合源帧、目标帧及音频采样要求，不静默取整、补帧或选择其他轨道。

保留的音轨按源时间戳对齐画面。音轨较晚开始或提前结束时，会按时间轴补静音并产生 `audio_silence` 警告。`if-present` 不表示无条件丢弃声音，只有显式 `drop` 才丢弃音轨。

VIDEO 保存运行画面及实际保留的音轨，原始 MP4/AVI 留在源码工程中。独立链接已有 `.hma` 不需要 FFmpeg。

## 输入边界

不支持变帧率、隔行、源旋转矩阵、非方形像素、HDR、透明视频或中途改变尺寸／格式。没有视频缩放参数，保持源逻辑尺寸。

源帧最多 10000 个；输入文件和运行 VIDEO 分别不超过 128 MiB，单帧解码 RGBA 不超过 64 MiB，全部解码 RGBA 不超过 128 MiB。

内嵌及外部 `.video` 的用法见[输入与部署](inputs.md)，播放器属性见[媒体播放控件](../../hmi-compiler/reference/controls-playback.md)。
