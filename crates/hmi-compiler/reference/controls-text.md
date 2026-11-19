# 文本与按钮

[项目首页](../../../README.md) · [页面编译器](../README.md) · [控件总览](controls.md)

`text`、`button`、`dual_state_button` 和 `scrolling_text` 均需要 `x/y/w/h`、`font` 和 `txt`，支持 `down/up` 事件；滚动文本另有自己的滚动参数。

## 字体和文本

```toml
[[objects]]
name = "title"
type = "text"
properties = { x = 10, y = 10, w = 280, h = 80, font = { ref = "ui_font" }, txt = "确定", txt_maxl = 10 }

[[objects]]
name = "confirm"
type = "button"
properties = { x = 10, y = 120, w = 160, h = 60, font = { ref = "ui_font" }, txt = "确定", style = 1 }
events = { up = "events/confirm.up.s" }
```

`ui_font` 须在工程资产表中声明为[字库资产](../../hmi-media/reference/fonts.md)。字体 ID 范围为 0～254，不接受 `none`；控件宽高不会改变字高。字号和字符集合由 ZI 字库决定。

`txt_maxl` 是 UTF-8 字节容量。例如“测试A”占 7 字节。文本、普通按钮和双态按钮默认容量为 10，滚动文本默认 64。

## 公共外观

| 公共属性 | 默认值与范围 |
|---|---|
| vscope、sendkey | 默认 0，可设 0／1 |
| bco、pco | RGB565 颜色值，0～65535；文本默认白底黑字，按钮普通状态 bco=50712、pco=0 |
| borderc、borderw | 默认 0、2，范围分别为 0～65535、0～255 |
| xcen、ycen | 默认 1；0 靠左／靠上、1 居中、2 靠右／靠下 |
| txt_maxl | 默认 10，范围 0～65535，单位为 UTF-8 字节 |
| isbr、spax、spay | 默认均为 0；isbr 为 0／1，spax、spay 为 0～255 |

文本的 `sta` 为 0 切图、1 单色、2 图片、3 透明；透明仅 X 系列支持。文本 `style` 为 0 平面／1 边框，默认 0；普通按钮 `style` 为 1 边框／4 3D_Auto，默认 4。背景与状态图片见[图片模式](controls-images.md)。

文本另有 `pw`，默认 0，可设 0／1。按钮按下状态颜色默认 `bco2=1024`、`pco2=65535`；`val` 为 0／1，默认 0。

## 双态按钮

`type = "dual_state_button"`。`val=0` 弹起、`val=1` 按下，默认 0；状态由控件自身切换，不需要额外生成切换脚本。`style` 为 0～4，默认 4；文字、字体、容量和颜色规则与按钮相同。

## 滚动文本

`type = "scrolling_text"`。默认 `style=0`、`xcen=0`、`ycen=1`，即平面、左对齐和垂直居中；没有普通文本的 `pw` 属性。

| 滚动属性 | 默认值与范围 |
|---|---|
| dir | 默认 1；0 左到右、1 右到左、2 上到下、3 下到上 |
| dis | 默认 4，范围 2～50，单位像素；不同于进度条或滑块的同名属性 |
| tim | 默认 80，范围 80～65534 毫秒；不能套用定时器控件的最小周期 |
| en | 默认 1，0 关闭／1 开启 |

```toml
[[objects]]
name = "notice"
type = "scrolling_text"
properties = { x = 20, y = 160, w = 280, h = 70, font = { ref = "ui_font" }, txt = "NOTICE", en = 0 }
```

`en` 控制运行时滚动，编辑器静态画布上的显示不代表实际滚动状态。
