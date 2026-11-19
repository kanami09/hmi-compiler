# 图片、背景与游标

[项目首页](../../../README.md) · [页面编译器](../README.md) · [控件总览](controls.md)

## 图片与切图控件

```toml
[[objects]]
name = "p0"
type = "picture"
properties = { x = 20, y = 20, w = 80, h = 60, pic = { ref = "logo" } }

[[objects]]
name = "q0"
type = "crop"
properties = { x = 20, y = 100, w = 80, h = 60, picc = { ref = "background" } }
```

两类均支持 `down/up`，`vscope/sendkey` 默认 0。普通图片使用 `pic`，切图使用 `picc`；切图源必须是与当前页面同尺寸的全屏图片，取图区域不能越界。

图片源和控件几何分别设置。修改控件宽高不会自动改变共享图片；需要缩放资产时使用[图片描述的 resize](../../hmi-media/reference/images.md#尺寸与透明度)。

普通图片引用不统一要求与控件宽高相同；实际显示由控件和运行环境决定。切图则明确要求全屏源尺寸和取图区域有效，这两种约束不能混用。

## 背景模式

| 控件 | 模式与必需图片 |
|---|---|
| page（页面自身） | sta=0 无背景、1 单色、2 全屏图片；sta=2 时 pic 必填，图片逻辑尺寸须与页面相同 |
| text／number／xfloat／scrolling_text | sta=2 时 pic 必填；sta=0 时 picc 必填；sta=1 单色；X 系列另支持 sta=3 透明 |
| button／dual_state_button | sta=2 时 pic／pic2 分别为普通／按下图片；sta=0 时 picc／picc2 为两种状态切图；sta=1 单色；X 系列另支持 sta=3 透明 |
| combo_box／text_select／sliding_text／file_browser／data_record | sta=2 时 pic 必填；sta=0 时 picc 必填；sta=1 单色、3 透明；五类控件当前仅在 X543 开放 |
| gauge／waveform | sta=2 时 pic 必填；sta=0 时 picc 必填；sta=1 单色、3 透明；指针透明仅 X 系列，波形当前按通用范围 |
| qrcode | sta=1 时 pic 为必填 LOGO；sta=0 不显示 LOGO，pic 可省略或保留 |
| slider | sta=2 时 pic／pic1 为背景／前景；sta=0 时 picc／picc1 为两项切图；sta=1 单色。独立的 psta=1 使用 pic2 游标图片 |
| progress_bar | sta=0 单色，sta=1 使用 bpic 背景与 ppic 前景；此类型的 sta 数值与文本等不同 |

`sta` 的数值含义由控件类型决定，尤其不能将进度条、二维码和页面自身的值套用到普通文字控件上。

```toml
[[objects]]
name = "b0"
type = "button"
properties = { x = 10, y = 10, w = 160, h = 60, sta = 2, font = { ref = "ui_font" }, txt = "OK", pic = { ref = "normal" }, pic2 = { ref = "pressed" } }
```

## 引用与保留规则

- 图片字段使用 `ref/id/none` 引用表。当前模式必须使用的图片不允许省略或为空。
- 切图图片必须与页面同尺寸，且取图区域有效；普通背景或 LOGO 不统一要求全屏。
- 显式声明的非活动引用仍然解析和保留，但不套用当前未启用的切图几何约束。
- 只切换模式不会删除图片配置，也不会清空其他类型的数值、文本、选项或事件。
- `sta=3` 表示透明背景，不填充背景；字体、边框、选中状态等仍按控件规则绘制。
- 透明背景、[公共不透明度](controls.md#拖动不透明度和加载特效)和[图片透明通道](../../hmi-media/reference/images.md#尺寸与透明度)是不同配置。

文件浏览器的 `pic1/pic2` 始终表示图标，与背景 `pic/picc` 不同；图标可省略以使用内置资源，但不能显式设为空。
