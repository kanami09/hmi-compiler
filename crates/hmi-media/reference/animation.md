# GIF 动画

[项目首页](../../../README.md) · [媒体编码器](../README.md) · [输入方式与部署](inputs.md)

GIF 源输入不需要 FFmpeg，支持局部帧、透明和 disposal 合成。

```toml
schema = "hmi.asset/1"
kind = "animation"
source = "loading.gif"

[encoding]
quality = 80
alpha = "preserve"
timing = "native-compatible"
```

| 字段 | 含义与默认行为 |
|---|---|
| source | 相对于资产描述文件的 GIF 路径；检查实际签名、逻辑画布、帧范围及数据块，按 GIF 语义合成局部帧和处理 disposal |
| encoding.quality | JPEG 运行帧质量，1～100、默认 80；100 仍使用 JPEG，不切换到私有编码 |
| encoding.alpha | 默认 preserve，保留透明度；flatten 时必须指定 background="#RRGGBB"，不改写原始 GIF |
| encoding.timing | 当前只支持 native-compatible：丢弃零延时帧，按原生规则处理不足 30ms 的短帧，单帧延时最大 65000ms |
| delivery | 默认 embedded；external 输出 sd0/ 下的 .gmov 运行文件，不占内部动画 ID |

## 时序与尺寸

不接受 `fps/codec` 或缩放参数。时序来自 GIF 延时，逻辑尺寸保持不变；控件的 `loop/en/dis` 不属于共享资产配置。

兼容时序策略可能丢弃零延时帧或调整短帧，产生 `animation_timing` 警告；`-Werror` 可阻止有该警告的发布。资产信息中的源帧数、输出帧数、时长、帧选择及延时记录用于查看实际调整。

原生包中无法恢复的源帧信息不作为原始输入事实。`native` 可保留已有运行资源，是否包含编辑源以包内实际成员为准；`encoded` 运行 GMOV 只用于外部方式。

## 输出与限制

默认内嵌，也可设置外部 `.gmov`，见[交付方式](inputs.md#内嵌与外部文件)。源码编码会保留原 GIF 作为编辑源，不部署编辑源到 SD。

最多 1000 个源帧，源文件不超过 64 MiB；单帧解码 RGBA 不超过 64 MiB，保留帧 RGBA 总量及运行资源分别不超过 128 MiB。纯文本 GIF 扩展不受支持。

播放控件用法见[动画与视频](../../hmi-compiler/reference/controls-playback.md#动画与视频)。
