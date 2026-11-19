# 控件参考

[项目首页](../../../README.md) · [页面编译器](../README.md) · [控件总览](controls.md)

## 阅读方式

先阅读[页面与事件](pages.md)了解 `objects` 的基本结构。下表列出支持的 30 类控件；型号是否开放某种控件，以[型号配置](../../../data/profiles/README.md)和当前目标的 `control_types` 为准。

| 分组 | 类型 | 说明 |
|---|---|---|
| [基础控件](#基础控件) | `hotspot`、`variable`、`timer`、`touch_cap` | 热区、变量、定时和触摸捕捉 |
| [文本与按钮](controls-text.md) | `text`、`button`、`dual_state_button`、`scrolling_text` | 文字、按钮和滚动文本 |
| [数值与交互](controls-numeric.md) | `number`、`xfloat`、`checkbox`、`radio`、`progress_bar`、`slider` | 数字、选择、进度和滑块 |
| [图形](controls-graphics.md) | `gauge`、`waveform`、`qrcode` | 指针、波形和二维码 |
| [选择与滑动](controls-selectors.md) | `switch`、`combo_box`、`text_select`、`sliding_text` | X543 的状态开关和文本交互 |
| [文件与记录](controls-files.md) | `file_browser`、`file_stream`、`data_record` | 文件浏览、文件流和数据记录 |
| [图片与背景](controls-images.md) | `picture`、`crop` | 图片、切图，以及其他控件的背景和游标 |
| [媒体播放](controls-playback.md) | `external_picture`、`animation`、`video`、`audio` | 外部图片、动画、视频和音频 |

## 公共规则

可视控件通常需要 `x/y/w/h`，位置为 0～65535，尺寸为 1～65535，几何端点不能超过 65535。非可视控件不接受这些字段。

- `vscope`：0 为私有，1 为全局；默认一般为 0，文件流和数据记录另有固定要求。
- `sendkey`：0／1，默认 0；只适用于支持该字段的控件。
- 颜色使用 RGB565 整数，范围 0～65535，不直接使用 `#RRGGBB` 字符串。
- 字体和图片使用[资源引用表](../../hmi-cli/reference/project.md#资源引用)，不写裸整数。
- 文本容量以 UTF-8 字节计，字库还需覆盖实际使用的字符；工具目前不逐字检查字形是否存在。
- `type/id/objname/endx/endy` 等生成字段不属于 `properties` 输入。
- 模式切换保留允许设置的非活动属性；隐藏配置不等于可以在所有模式下使用。

同名字段在不同控件中的含义可能不同。例如 `dis` 可表示圆角比例、移动步长或播放速度，应按对应类型解释。

## 拖动、不透明度和加载特效

| 属性 | 默认值与范围 | 含义 |
|---|---|---|
| `drag` | 0，取 0／1 | 是否允许拖动 |
| `aph` | 127，范围 0～127 | 不透明度；0 完全透明，127 完全不透明 |
| `effect` | 0，范围 0～8 | 加载特效；0 立即显示，1～8 为不同方向飞入 |

X543 对支持这些字段的控件开放上述范围。T0/K0 限制为 `drag=0`、`aph=127`、`effect=0`。页面只支持 `effect`；热区和非可视类型不自动获得这些属性。

`aph=0` 与背景模式 `sta=3` 是不同功能，不能互相替代。对未支持的控件填写这些字段会报错。

## 基础控件

| 类型 | 主要属性 | 事件 |
|---|---|---|
| `hotspot` | 必填 `x/y/w/h`；`vscope/sendkey` 默认 0 | `down`、`up` |
| `variable` | `sta=0` 数值／`sta=1` 字符串；`val` 为 i32，默认 0；`txt` 默认 `newtxt`；`txt_maxl` 默认 10 | 无 |
| `timer` | `tim` 默认 400，范围 50～65534 毫秒；`en` 默认 1；`vscope` 默认 0 | `timer` |
| `touch_cap` | 只接受 `vscope`；每页最多一个；`val` 为运行时只读结果 | `down`、`up` |

```toml
[[objects]]
name = "counter"
type = "variable"
properties = { sta = 0, val = -7, vscope = 1 }

[[objects]]
name = "tap"
type = "hotspot"
properties = { x = 20, y = 30, w = 120, h = 60 }
events = { up = "events/tap.up.s" }

[[objects]]
name = "tm0"
type = "timer"
properties = { tim = 400, en = 0 }
events = { timer = "events/tm0.timer.s" }
```

变量和定时器没有可见几何。变量切换 `sta` 不删除另一种模式的初始配置；运行脚本中，数值模式使用 `val`，字符串模式使用 `txt/txt_maxl`。

触摸捕捉的 `val` 初始化为 0，不能作为初始属性输入；脚本可读取如 `sys0=tc0.val`。实际捕捉结果由运行环境产生。
