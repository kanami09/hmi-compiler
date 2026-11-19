# WAV 音频

[项目首页](../../../README.md) · [媒体编码器](../README.md) · [输入方式与部署](inputs.md)

源输入为 WAV。参数不变的 PCM16 可直接处理；位深、采样率或声道转换需要 [FFmpeg 后端](ffmpeg.md)。

```toml
schema = "hmi.asset/1"
kind = "audio"
source = "startup.wav"

[encoding]
sample_rate = 8000
channels = 1
sample_format = "s16"

[clip]
start_ms = 250
duration_ms = 500
```

| 字段 | 默认与含义 |
|---|---|
| source | 相对资产描述文件的 WAV 输入，检查 RIFF、fmt、data 与块边界；支持 PCM 8/16/24/32 位和浮点 32/64 位，其他压缩 WAV 暂不支持 |
| encoding.sample_rate | 省略保持源采样率；当前输出支持 8000、11025、22050、24000、32000、44100Hz。不支持的源参数需显式指定转换，不自动选值 |
| encoding.channels | 省略保持源声道数；输出支持 1／2。多声道源转换需提供明确的输入布局；不猜测未知多声道的扬声器位置 |
| encoding.sample_format | 默认且当前只支持 s16，有符号 16 位小端 PCM；不存在图片的 quality、fps 或 resize 参数 |
| clip | 起点默认 0，duration 省略到结束。按转换后的时间轴截取，起点／时长须能精确表示为输出采样，越界不裁剪或悄悄补零 |
| delivery | 默认 embedded；external 输出 sd0/ 下的格式化 .wav，不占内部音频 ID |

## 转换与截取

音频没有屏幕方向，同一资产目标可用于四个工程方向。处理不改写源 WAV，不接受图片的 `quality/fps/resize`。

多声道转换需要明确输入布局，不能猜测扬声器位置。采样率或声道不在输出支持范围内时，应明确设置转换参数。

`clip` 作用于转换后的时间轴，边界必须能表示为完整输出采样帧。运行文件按要求对齐时可能补入一个静音采样帧；`content_samples`、`sample_frames`、`padding_samples` 分别记录有效数量、输出总数和补入值。

`duration_ms` 为实际输出时长的整数毫秒部分，精确时长可由 `sample_frames/sample_rate` 计算。原文件和输出 PCM 分别不超过 64 MiB。

## 交付与播放

默认内嵌，也可输出外部格式化 `.wav`。设备运行文件含 HMI 所需包装，不能仅将任意 WAV 改名当作已编码输入。已有格式化 WAV 可用 `encoded` 封装，见[输入模式](inputs.md)。

音频控件与视频内音轨分别管理，控件配置见[媒体播放](../../hmi-compiler/reference/controls-playback.md#音频)。
