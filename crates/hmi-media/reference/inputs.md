# 媒体输入与交付方式

[项目首页](../../../README.md) · [媒体编码器](../README.md) · [输入方式与部署](inputs.md)

## 资产描述与资产符号

工程资产表中的 `source` 指向描述文件；描述文件中的 `source` 才指向原图、字库、GIF、视频、WAV 或已有资产包。

```toml
# hmi.toml
[assets]
items = [{ key = "logo", source = "assets/logo.asset.toml" }]
```

```toml
# assets/logo.asset.toml
schema = "hmi.asset/1"
kind = "image"
input = "source"
source = "images/logo.png"
```

本例原图位于 `assets/images/logo.png`。控件使用 `{ ref = "logo" }` 引用该资产，而不是引用文件名。资产符号与类别编号见[工程文件参考](../../hmi-cli/reference/project.md#页面控件和资产)。

## 输入方式

| 配置 | 输入及处理行为 |
|---|---|
| `input = "source"` | 默认方式，按明确参数处理图片、ZI、GIF、受支持的视频或 WAV |
| `input = "encoded"` | 检查已编码运行文件的签名、结构、方向和目标兼容性，封装为资产包，不重新压缩 |
| `input = "native"` | 检查并保留支持的 `.hma` 包及其成员，不要求原始媒体或 FFmpeg |
| `source` 加 `original` | 源仍为必需输入，只有内容摘要、处理请求和目标条件一致时才复用已有编码 |

### 已编码输入

```toml
schema = "hmi.asset/1"
kind = "image"
input = "encoded"
source = "logo.xi"
```

支持已知 JPEG 分支的 XI、方向匹配的 ZI、VIDEO、已格式化 PCM16 WAV，以及供外部部署的运行 GMOV。普通 JPG 不作为 XI 的替代输入，应使用 `source` 编码。

运行 GMOV 的 `encoded` 输入只支持 `delivery.mode="external"`。内嵌动画使用 GIF 源或支持的原生包。ZI 的 `encoded` 输入不自动转方向，需要适配时使用 `source`。

`encoded` 不接受 `encoding/resize/clip/audio/original` 等重新处理参数。格式根据实际内容识别，不能用改后缀代替编码。

### 原生资产包

```toml
schema = "hmi.asset/1"
kind = "video"
input = "native"
source = "../.preserve/assets/video_00000.hma"
```

包的类别必须匹配 `kind`，格式、方向及型号也必须兼容。默认保留包的内嵌／外部方式，不通过 `native` 切换交付模式；外部包可以显式更改 `delivery.path`。

包内已有的编辑源随包保留，但不是另一份隐式输入。包中没有的图片或 GIF 编辑源不会被伪造。无法恢复的 JPEG 原质量等信息保留为未知，不填入猜测值。

### 条件复用原始编码

```toml
schema = "hmi.asset/1"
kind = "image"
source = "images/logo.png"
original = "../.preserve/logo.hma"

[encoding]
codec = "jpeg"
quality = 80
```

`original` 是显式声明的资产包附件。源内容、有效处理参数及相关目标条件一致时复用编码，否则按当前 `source` 重新处理。`codec="auto"` 会按目标展开后比较；音频不以屏幕方向作为条件。

源和附件都必须存在且有效。附件不能代替缺失源，也不会在附件损坏时转而读取查看副本。包内没有可比较的原始请求时，按正常源输入处理。命中时可以无需 FFmpeg，实际重新转换时仍需对应后端。

## 内嵌与外部文件

默认 `delivery.mode="embedded"`。选择外部方式时，必须设置安全的设备路径：

```toml
[delivery]
mode = "external"
path = "sd0/media/intro.video"
```

| 资产类别 | 外部文件后缀 |
|---|---|
| 图片 | `.xi` |
| 动画 | `.gmov` |
| 视频 | `.video` |
| 音频 | `.wav` |

字体使用内嵌方式。外部资产不占对应类别的内嵌资源 ID；显式声明的外部资产即使未被控件使用，也会输出配套文件。

## 外部文件与设备路径

`delivery.path` 是屏幕读取的位置，自动输出限定在 `sd0/` 下。不接受本机绝对路径、反斜杠、`.`、`..`、非法文件名或错误的文件后缀。

若 HMI 输出为 `dist/Panel/Panel.HMI`，上述视频会写到 `dist/Panel/sd0/media/intro.video`。应将 `sd0/` 内的内容按相同目录结构部署到 SD 卡或上位机虚拟 SD。

控件的 `path` 有两种用法：

- `{ ref = "intro" }`：绑定工程内对应类别的外部资产，由工具输出配套文件。
- `"sd0/media/existing.video"`：只声明已有的运行文件，不读取、生成或检查构建机同名文件。设备支持的 `ram/` 等路径也属于这一类。

工具不会自动改动 SD 卡或上位机虚拟 SD。复制普通 AVI/GIF 并改后缀不会得到运行媒体。多个资源路径冲突、父子路径冲突或不兼容的数据文件需求会报错。

`link-map.json` 的 `runtime_files` 记录初始路径、使用控件、结构及是否由本次生成。它不预测运行脚本后续改写路径的结果。

## 探测、目标文件与失败处理

`probe` 不写输出，但 `source` 输入可能完整解码；`encode` 执行实际处理并写 `.hma`；`inspect` 查看资产包的描述和成员。

`.hma` 自包含运行数据和实际已有的编辑源，独立链接不需要原始媒体或转换后端。方向适配只作用于需要方向的媒体，源文件不会被改写。

输出不能覆盖声明的输入。各命令的执行结果、输入处理方式 `input_action` 和发布状态见[自动化接口](../../hmi-cli/reference/automation.md)。
