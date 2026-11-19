# HMI文件格式

[项目首页](../../README.md) · [公共库导航](../../README.md#公共库)

`hmi-format`（Rust 库名 `hmi_format`）提供HMI容器、页面及媒体资源的结构读取、写入和校验，供解析器、编译器、产物库及链接器复用。

## 主要内容

| 模块职责 | 说明 |
|---|---|
| 容器与工程 | 目录、活动资源、主工程引用及整体结构。 |
| 页面与对象 | 对象记录、属性、事件和页面组织。 |
| 媒体资源 | 图片、ZI、GMOV、VIDEO、WAV的已支持格式。 |
| 写入与校验 | 生成结构并核对长度、边界、摘要或对应校验值。 |

## 职责边界

不管理源码工程、缓存和任务调度，也不加载 FFmpeg。公开模块以Rust API 文档为准；格式支持边界由相应工具说明。

## 开发入口

在工作区根目录查看 API 文档：

```powershell
cargo doc -p hmi-format --no-deps --open
```

各命令行工具调用共享库，不要求通过子进程复用。应用程序可按需要依赖该包；具体类型、函数和模块以生成的 Rust 文档为准。

[工具使用总览](../../README.md#文档导航) · [工程文件](../hmi-cli/reference/project.md) · [页面编译](../hmi-compiler/README.md) · [媒体处理](../hmi-media/README.md)
