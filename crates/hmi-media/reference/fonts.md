# ZI 字库

[项目首页](../../../README.md) · [媒体编码器](../README.md) · [输入方式与部署](inputs.md)

```toml
schema = "hmi.asset/1"
kind = "font"
source = "fonts/ui.zi"
```

## 字号、字符与方向

输入须为受支持的 UTF-8 ZI v6，支持灰度 RLE 字形及透明空白字形。其他版本、非空白单色字形或未识别布局会报错。本工具不从 TTF 制作字库。

字高和字符集合由 ZI 决定，不能通过字体 ID 或控件宽高改变字号。需要不同字号时提供不同 ZI；使用的文字还必须包含在字库字符集合中。

`source` 输入不需要方向选项。编码器自动适配工程的 0°、90°、180° 或 270°，不改写源 ZI。`encoded/native` 输入则需要已经兼容目标方向；要转换方向应使用 `source`。

字库不接受图片的 `quality/alpha/resize` 参数，交付方式为内嵌。

## 工程中的引用

```toml
# hmi.toml
[assets]
items = [{ key = "ui_font", source = "assets/ui_font.asset.toml" }]
```

控件中使用 `font = { ref = "ui_font" }` 或对应字体列表的 `{ id = 0 }`。合法字体 ID 为 0～254，不能设为空，也不能将图片 ID 当作字体 ID。

多个控件可共享字库，不同类别资产分别编号。`probe` 检查索引和压缩数据，不输出方向转换结果；独立链接只需对应 `.hma`。

字库输入上限为 64 MiB，解码总像素数上限为 128 × 1024 × 1024；文本容量另按 UTF-8 字节检查，见[文本控件](../../hmi-compiler/reference/controls-text.md)。
