# 页面、事件与脚本

[项目首页](../../../README.md) · [页面编译器](../README.md) · [工程清单](../../hmi-cli/reference/project.md) · [控件参考](controls.md)

## 页面文件

```toml
schema = "hmi.page/1"
name = "home"

[properties]
sta = 1
bco = 65535

[events]
load = "events/home.load.s"

[[objects]]
name = "counter"
type = "variable"
properties = { sta = 0, val = 0, vscope = 1 }

[[objects]]
name = "tap"
type = "hotspot"
properties = { x = 20, y = 30, w = 120, h = 60 }
events = { up = "events/tap.up.s" }
```

| 字段 | 含义 |
|---|---|
| `schema` | 页面格式，填写 `hmi.page/1` |
| `name` | 页面当前名称，遵循[工程名称规则](../../hmi-cli/reference/project.md#页面控件和资产) |
| `properties` | 页面自身的背景、颜色、图片和加载特效 |
| `events` | 页面事件槽与脚本路径的对应关系 |
| `objects` | 有序控件数组；顺序决定图层和从 1 开始的控件 ID |
| `objects[].name` | 当前控件名称，页内唯一 |
| `objects[].type` | 控件类型，见[控件总览](controls.md) |
| `objects[].properties` | 该类型允许设置的属性 |
| `objects[].events` | 该类型允许的事件槽和脚本路径 |
| `original_record` | 导入工程的原始记录定位；新建工程通常省略，见[解析器说明](../../hmi-parser/README.md) |

页面自身 ID 始终为 0。`type`、`id`、`objname` 以及几何端点由工具生成，不能再放入控件的 `properties`。

## 页面背景

页面几何由目标型号和方向决定：x/y 为 0，w/h 取目标画布，页面源码不能覆盖这些值。

| 属性 | 默认值与含义 |
|---|---|
| `sta` | 默认 1；0 无背景、1 单色、2 图片 |
| `bco` | 默认 65535，RGB565 颜色，范围 0～65535 |
| `pic` | 图片引用；图片背景必填，图像逻辑尺寸必须与页面相同 |
| `effect` | 默认 0；X543 支持 0～8，T0/K0 仅 0 |

```toml
[properties]
sta = 2
pic = { ref = "background" }
```

页面背景的 `sta` 与普通文本控件的背景数值不同，也不支持 `sta = 3`。无背景表示不主动绘制背景，实际画面取决于运行环境。

非活动图片配置可以保留；从图片切换为单色不会自动删除 `pic`。省略 `pic` 表示空引用，不会默认绑定图片 ID 0。

## 事件文件

页面支持 `load`、`loadend`、`down`、`up`、`unload`。控件按类型提供各自事件，未引用的槽为空。

事件脚本必须使用相对路径并以 `.s` 结尾，路径相对于页面文件目录。每个被引用的文件都必须存在：

```text
pages/home/page.toml
pages/home/events/home.load.s
pages/home/events/tap.up.s
```

例如 `tap.up.s`：

```text
counter.val+=1
```

脚本与[全局代码](../../hmi-cli/reference/project.md#全局代码和事件)均使用不带 BOM 的 UTF-8。文本控件中的换行、字节容量与事件脚本是不同规则，见[文本控件](controls-text.md)。

## 引用与检查

`home.b0.txt` 按当前页面和控件名称检查；名称组中的常量 ID 按最终 ID 检查。跨页访问还受 `vscope` 和属性本身的访问范围约束。

当前脚本检查主要覆盖引用、作用域、变量模式和部分属性读写。动态表达式可能只能给出警告，指令和控件方法参数、完整语法及类型检查尚未覆盖。独立页面编译成功不等于完整工程的脚本检查通过；构建结果仍应交由 USART HMI 编译。
