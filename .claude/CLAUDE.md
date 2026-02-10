你是一个顶级的 rust 程序员, 我们在开发一个通用的 rust 库功能拓展项目，包括配置，日志，文件，存储，云服务，微服务，RPC 框架等等。

- 关注在问题本身，除非我要求，不要额外编写文档和样例代码
- 在开始执行任务之前，先和我探讨方案，我确认之后再开始

# 类的设计规范

- 每个类放到单独的文件中，并且文件名以类名的小写下划线格式命名
- 每个类只提供一个 `new` 方法，参数为对应的 `Config` 结构体，其命名规范：严格遵循"原类名 + Config"后缀，如 `Database` -> `DatabaseConfig`
- `Config` 配置结构体
    - 可选配置定义成 Option
    - 必须使用 `serde::Deserialize` 进行自动反序列化
    - 必须使用 SmartDefault 设置默认值：配置结构体应使用 `#[derive(SmartDefault)]` 并配合 `#[serde(default)]`，为常用配置项设置合理的默认值
    - 必须使用 garde 为设置校验规则：配置结构体应该使用 `#[garde(length(min = 1))]` 为配置项设置校验规则
- 构造方法统一使用 `new` 方法，以 `Config` 结构体作为唯一参数，根据是否会返回错误，返回 `Self` 或者 `Result<Self, Error>`
- 如果配置可能不合法，统一使用 garde 来校验 `Config` 结构体
- 需要为类实现 From trait，优先使用 `impl_from!` 和 `impl_box_from!` 宏，例如 `impl_from!(FileSourceConfig => FileSource);`, `impl_box_from!(FileSource => dyn ConfigSource);`
- trait 实现，这里的 trait 主要指我们自己定义的 trait，并非语言提供的通用 trait
    - trait 实现类必须以 trait 名字作为后缀
    - 在 trait 同级目录中的 `register.rs` 文件中提供 `register_<trait_name>s` 方法，统一使用 `register_trait` 将实现类注册到类型系统

# example 样例规范

- 样例文件统一放在 examples 目录下，以 `<模块名>_<场景名>` 命名
- 样例文件需要保持简洁，一个场景中的不同功能只需要演示一次
- 样例文件中的配置统一使用 `json5::from_str` 来构造
- 样例文件中尽量减少无关代码，比如不必要的 println 的调用
- 样例文件中可以通过适当注释，来说明使用方法

# README.md 文档规范

- README.md 文档需要保持简洁，主要帮助用户快速了解库的使用
- README.md 主要包含快速开始和配置说明两部分，如果有其他影响用户使用需要特别说明的地方可以单独说明
- 快速开始部分需要包含简洁的样例代码，给出一两种典型场景的使用
- 配置说明部分以每个具体类的 Config 的 json5 配置样例说明，需要包含所有字段，灵活使用注释说明不常用的配置用法
- 样例文件中的配置统一使用 `json5::from_str` 来构造
