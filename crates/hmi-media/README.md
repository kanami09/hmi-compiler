# 图片编码器

当前支持 BMP、PNG、JPG 输入，生成 K/T 配置使用的 RGB565 图片。X 系列 JPEG、字库、动画、视频、音频、外部文件及原生载荷复用尚未接入。

```powershell
hmi-media probe image.asset.toml --target target.json
hmi-media encode image.asset.toml --target target.json -o image.hma
hmi-media inspect image.hma
```

输入格式根据文件签名识别，不能通过改后缀转换格式。probe 完整解码并检查处理参数，但不生成运行像素或输出文件。当前解码后的 RGBA 数据最多为 64 MiB。

## 描述文件

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

source 相对于描述文件所在目录。codec 可省略，或设置为 auto、rgb565；RGB565 不接受 quality 参数。delivery 默认 embedded，目前不支持 external。

alpha 默认 preserve。RGB565 不支持透明像素，输入存在透明度时会报错；需要去透明时设置 flatten，并指定 #RRGGBB 背景色。resize 默认不启用，启用时宽高必须同时给出：stretch 拉伸；contain 保持比例并居中填充，且须提供 #RRGGBBAA 背景。填充背景不会隐式覆盖主体透明度，仍由 alpha 策略决定处理方式。

处理只生成派生资源，不改写原图。无需转换的标准 24 位 BMP 保留原始编辑源字节；PNG/JPG、缩放或去透明后的编辑源统一保存为 BMP。工程 source 仍指原始输入，.hma 中的 source 成员表示上位机编辑源，不保证是原文件的字节副本。

## 目标文件

.hma 包含版本化 JSON 描述、运行图片和编辑源，并对完整内容及成员计算摘要。逻辑宽高不因存储方向改变，90°／270° 的存储光栅宽高单独记录。链接器校验编码、方向和格式兼容性，不要求源文件继续存在。

`.i` 运行数据使用小端 RGB565，颜色转换直接截去低位；`.is` 保存 BMP 编辑源。只有运行成员占用图片 ID，编辑源不会单独占 ID，也不进入主引用表。图片 ID 和内部资源名由链接器分配。

所有工具支持 --json。输出不能覆盖声明的输入，编码失败不会更新已有输出。
