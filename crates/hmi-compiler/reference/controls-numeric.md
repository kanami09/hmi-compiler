# 数值、选择与进度

[项目首页](../../../README.md) · [页面编译器](../README.md) · [控件总览](controls.md)

本页控件均支持 `down/up`，`vscope/sendkey` 默认 0。数字与虚拟浮点数需要字库；复选框、单选框和单色进度条、滑块不需要媒体资产。

## 数字与虚拟浮点数

```toml
[[objects]]
name = "n0"
type = "number"
properties = { x = 10, y = 10, w = 280, h = 70, font = { ref = "ui_font" }, val = -12 }

[[objects]]
name = "x0"
type = "xfloat"
properties = { x = 10, y = 90, w = 280, h = 70, font = { ref = "ui_font" }, val = -1234, vvs1 = 2 }
```

`val` 均为有符号 32 位整数，范围 -2147483648～2147483647，默认 0。`xfloat` 根据小数位显示整数，例如 `val=-1234, vvs1=2` 显示 -12.34；不能写 TOML 浮点数或 IEEE 浮点位模式。

| 类型 | 格式字段 |
|---|---|
| `number` | `lenth` 为 0～15，默认 0 自动位数；`format` 为 0 数字／1 货币／2 Hex，默认 0 |
| `xfloat` | `vvs0` 为 0～10，默认 0 自动整数位；`vvs1` 为 0～8，默认 2，小数位设为 0 时无小数 |

属性名使用 `lenth`，不接受 `length` 别名。两类的 `style` 为 0～4，默认 0，`isbr` 默认 1，支持颜色、边框、对齐、间距和[背景模式](controls-images.md)，没有 `txt/txt_maxl/pw`。

## 复选框与单选框

`checkbox` 和 `radio` 的 `val` 为 0 未选中／1 选中，默认 0。默认 `bco=65535`、`pco=1024`。复选框另有 `style`（0～4，默认 2）、`borderc`（默认 0）和 `borderw`（默认 2）；单选框不接受这些字段。

单选框不会自动生成互斥逻辑，需要在事件中设置其他单选框的 `val`。

## 进度条

`type = "progress_bar"`。`val` 为 0～100，默认 0；`dez=0` 横向、`dez=1` 竖向，默认 0。`sta=0` 单色，`sta=1` 图片，使用 `bpic/ppic`；不要套用文字控件的 `sta` 数值。

默认背景 `bco=50712`、前景 `pco=1024`。`dis` 为圆角比例，0～100，默认 25；外观随型号而异，工具保留该配置，不生成替代图形。

## 滑块

```toml
[[objects]]
name = "h0"
type = "slider"
properties = { x = 20, y = 20, w = 280, h = 40, minval = 0, maxval = 100, val = 40 }
events = { slide = "events/h0.slide.s" }
```

支持 `down/up/slide`。`mode=0` 横向、`mode=1` 竖向，默认 0。`val/minval/maxval` 均为 0～65535，默认 0／0／100，且 `minval <= maxval`。最小／最大值限制滑动，不据此拒绝范围外的显式初值，也不自动夹紧 `val`。

`sta=0` 切图、1 单色、2 图片，默认 1。背景与游标图片见[图片模式](controls-images.md)。

| 滑块属性 | 默认值与范围 |
|---|---|
| psta | 默认 0；通用范围为 0／1／2，K0 已确认限制为 0／1。1 为图片游标，必须提供 pic2 引用 |
| wid、hig | 默认 255，范围 0～255；255 表示自动，0 表示无游标 |
| dis、ch | 默认 100、25，范围 0～100；分别为圆角比例、背景宽度比例 |
| bco、bco1、pco | 默认 50712、1024、63488，范围 0～65535；分别为背景、前景和游标颜色 |

K0 的 `psta` 仅允许 0／1；X543 和当前 T0 配置使用通用 0／1／2 范围。`psta=0` 的实际外观可随型号变化，不能仅凭相同字段保证相同绘制效果。
