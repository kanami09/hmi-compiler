# Workspace 编辑规则

## 文件与定位

```text
workspace/
├── project.toml             # 只读的配置和页面列表
├── program.s                # 可编辑的全局代码
├── pages/<页面目录>/
│   ├── page.toml            # 页面及控件属性、事件路径
│   └── events/*.s           # 事件脚本
├── assets/images/           # 只读图片预览
├── assets/fonts/            # 只读字库
├── .hmi/                    # 原始工程及保真映射，不修改
└── .build/                  # 自动生成的构建缓存
```

`project.toml` 的 `pages` 是相对于 workspace 的页面目录列表。目录名经过转义，不应由显示名称推算。`page.toml` 顶部的 `name`、`[properties]` 和 `[events]` 属于页面自身，每个 `[[objects]]` 属于一个控件。

保留页面和控件的名称、类型、顺序、数量及属性集合。仅修改支持编辑的现有属性值；未知字段、只读字段、`raw:...` 值保持原样。不得通过改动 `.hmi/`、资源预览或 `project.toml` 实现结构变更。

## 属性

导出文件可能使用 TOML 1.1 多行内联表；Python 标准库 `tomllib` 等仅支持 TOML 1.0 的解析器可能拒绝它。保留现有排版做局部修改，或使用支持 TOML 1.1 的工具，最终以 `hmi-compiler check` 校验；不要因为辅助解析器报错就重写整个文件。

下面只是某个已有控件的局部示意，实际以导出字段为准，不据此创建控件或补齐不存在的属性：

```toml
[[objects]]
name = "b0"
type = "button"
properties = {
    x = 20, y = 20,
    w = 100, h = 40,
    txt = "确定",
    txt_maxl = 6,
}
events = { up = "events/o001_b0.up.s" }
```

| 属性 | 编辑约束 |
|---|---|
| `txt` / `txt_maxl` | 容量按 UTF-8 字节数计算，不按字符数；容量为 `0..65535`。例如“确定”占 6 字节。文本变长时在需要且允许时同步调整容量。 |
| `x/y/w/h` | 尺寸大于零，派生端点不超过 `65535`；结合原工程显示尺寸判断布局。 |
| `vscope` | `0` 为私有，`1` 为全局；跨页面访问需考虑作用域。 |
| `font` | 已有字库 ID；不自行生成字库或假定任意字库包含中文字形。 |
| `pic/pic2/picc/picc2` 等 | 已有图片 ID，`65535` 表示无图片；不能通过修改导出图片文件替换资源。 |
| `bco/bco2/pco/pco2/borderc` 等 | 保持原字段的整数表示；不要写 CSS 颜色字符串。 |
| `val` | 双态按钮、复选框、单选框为 `0/1`；变量控件支持有符号 32 位，其余已支持类型按存储宽度校验。 |
| `tim/en` | 定时器周期 `50..65534` 毫秒；`en` 为 `0/1`。 |

## 事件与全局代码

事件路径相对于所属页面目录。编辑映射引用的 `.s` 文件；仅创建文件而不添加引用不会启用事件。删除引用或清空文件表示清空对应事件。添加引用只适用于控件本身已有的事件槽位，不任意增加事件种类。文件保持 UTF-8。

| 事件键 | 含义 |
|---|---|
| `down/up` | 触摸按下/弹起 |
| `load/loadend` | 页面初始化前/后 |
| `unload` | 页面离开 |
| `timer` | 定时触发 |
| `slide` | 滑块滑动 |

`.s` 是 USART HMI 脚本，不是 CPU 汇编、Rust 或 C。保持原有设备语法和上下文，参考相邻代码；全局代码位于 `program.s`。静态检查覆盖部分引用和格式约束，不能证明任意脚本语法、运行行为、显示效果或资源字形正确。未知指令应查阅相应版本的官方资料或向用户核实，不能仅因 `check` 成功就声称有效。

## 诊断与差异

JSON 结果包含 `schema_version`、`ok`、`result` 和 `diagnostics`。诊断有 `severity`、`code`、`message`、`location`，并可能带 `category`、`original_severity`。`location` 可包含 `file`、`line`、`page`、`object`、`property`、`event` 等，按实际存在的字段定位。

常见警告包括页面/控件/属性不存在、跨页面访问私有对象、动态目标无法静态确认、图片或字库 ID 无效。`-Werror` 可用于用户要求的严格检查；动态表达式类警告需要结合上下文判断。`encoding`、`resource-layout` 警告不会因关闭显示而获得格式支持。

`diff` 的 `result.changes` 各项包含 `kind`、`resource`、`before`、`after`，以及可能存在的页面、控件和字段。`byte_identical` 表示整个文件逐字节相同；`active_resources_equal` 表示活动资源内容相同；`container_changed` 表示容器变化。用实际差异核对修改范围。

工具基于 USART HMI `v1.68.2` / 文件版本 `ver21234` 开发。项目当前验收记录为离线回归通过、官方软件打开和编译验收待完成；交付新工程时应准确报告本次实际验证范围。
