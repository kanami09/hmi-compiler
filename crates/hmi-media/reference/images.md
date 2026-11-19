# 图片资产

[项目首页](../../../README.md) · [媒体编码器](../README.md) · [输入方式与部署](inputs.md)

## 支持范围

源图片支持 BMP、PNG、JPG。T0/K0 配置使用 RGB565，X543 使用 Baseline JPEG YUV420，并可保留七位透明度。源格式按签名识别，不靠后缀转换。

```toml
schema = "hmi.asset/1"
kind = "image"
source = "images/logo.png"

[encoding]
codec = "auto"
alpha = "flatten"
background = "#FFFFFF"

[resize]
width = 72
height = 48
mode = "stretch"
```

## 编码参数

| 字段 | 默认与含义 |
|---|---|
| `encoding.codec` | 默认 `auto`，按目标选择 `rgb565` 或 `jpeg`；也可明确指定目标支持的路线 |
| `encoding.quality` | JPEG 为 1～100，默认 80；RGB565 不接受此字段 |
| `encoding.alpha` | 默认 `preserve`，也可设 `flatten` |
| `encoding.background` | `flatten` 时必填 `#RRGGBB`，用于合成不透明背景 |
| `resize.width/height` | 缩放时必须同时设置，未配置则保持尺寸 |
| `resize.mode` | `stretch` 拉伸；`contain` 等比缩放并居中填充 |
| `resize.background` | `contain` 必填 `#RRGGBBAA`，控制填充区域 |

`quality=100` 仍是 JPEG，不保证无损。质量只影响颜色编码，不增加透明度位数。

## 尺寸与透明度

图片资产的尺寸与控件几何独立。共享一张图的多个控件不会各自产生不同缩放结果；若需要不同尺寸资产，应分别声明描述文件和 `resize`。

`preserve` 时不设置编码背景色。X543 保留变换后的透明度；RGB565 遇到透明像素会报错，需要先使用 `flatten`。填充区域与主体都按 `alpha` 策略处理。

运行透明度由八位量化为七位，会有精度损失；保留的 PNG 编辑源仍使用八位透明度。上位机显示 PNG 标签，不表示运行颜色数据采用 PNG 编码。

工具自动适配四个工程方向，逻辑尺寸不因存储方向改变。内部补齐画布也不会拉伸图片或改变控件几何。切图使用与页面同尺寸的全屏资产，详细模式要求见[图片控件与背景](../../hmi-compiler/reference/controls-images.md)。

## 内嵌与外部

默认内嵌，可选外部 XI：

```toml
[delivery]
mode = "external"
path = "sd0/pictures/logo.xi"
```

XI 支持已知 JPEG 编码分支。普通 JPG 的直接运行路径由设备处理；要让本工具生成外部文件，使用上述资产编码方式。外部部署和路径引用见[交付规则](inputs.md#外部文件与设备路径)。

## 编辑源和边界

源输入仍指原图。`.hma` 中的编辑源用于上位机：不透明结果通常为 BMP，保留透明度时为 RGBA PNG，不保证与原图逐字节相同。符合要求且无需转换的标准 BMP 可保留原字节。

单帧解码 RGBA 数据上限为 64 MiB；变换或补齐后超限会报错。已编码 XI 和原生包的保留规则见[输入模式](inputs.md#输入方式)。
