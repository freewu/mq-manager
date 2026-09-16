# mq-manager

使用 Tauri 2 + React + Vite 开发的一套通用的消息队列管理客户端。

连接层是「驱动 + 中立模型」的结构：界面和命令层不认识任何具体的 broker，只认识
`TopicSummary`、`Message`、`ConsumerGroupSummary` 这类中立结构，具体能力由每个驱动
通过 `Capabilities` 声明。因此新增一种消息队列 = 新增一个驱动模块 + 注册一行。

## 支持的 Broker

| 驱动 | 协议 | 说明 |
| --- | --- | --- |
| Apache Kafka | Kafka 协议（`rdkafka`） | 主题 / 分区 / 消费组 / 位点 / 生产 / 非破坏性浏览 / 实时 tail |
| Apache RocketMQ | RocketMQ remoting 协议（自研实现） | 主题（读写队列）/ 订阅组 / 位点 / 生产 / 浏览 / tail；不支持 purge |
| RabbitMQ | AMQP 0-9-1（`lapin`）+ Management HTTP API | 队列 / 交换机 / 消费者 / 生产 / 浏览 / tail；队列参数即 AMQP arguments |
| Demo | 进程内假数据 | 无 broker 也能完整体验界面 |

### 浏览（Browse / Tail）不会消费消息

- Kafka：使用独立的 group 与 `assign`（不提交位点）读取消息。
- RabbitMQ：通过管理插件的 `POST /api/queues/{vhost}/{name}/get`，且固定使用
  `ack_requeue_true`；tail 是对队首的轮询 + 指纹去重，不会 ack 任何消息。
- RocketMQ：用一个临时订阅组按队列 pull，从不提交位点（`commitOffset=0`）。

### 依赖 Management Plugin（RabbitMQ）

AMQP 协议本身无法「列出所有队列」，所以 RabbitMQ 驱动的发现能力来自
`rabbitmq_management`（默认 15672 端口）。连接配置里的 `managementApi=false`
可以把连接退化成「只能生产」；端口不可达时连接会直接失败，并给出可执行的提示。
连接需要带 `management` tag 的账号。

## 环境要求

- Rust **1.88+**（`lapin` 4.x 的要求）
- Node.js 20.19+、pnpm 11
- Tauri 2 的系统依赖（Windows 需要 MSVC 工具链与 WebView2）
- 可选：Docker，用于下面的本地测试环境

TLS 使用 `rustls` + `ring`（不使用 `aws-lc-rs`，因此不需要 CMake / NASM）。

Linux 上编译 Rust 侧还需要 `libcurl4-openssl-dev`（Kafka 驱动依赖的 librdkafka 会无条件
`#include <curl/curl.h>`，虽然并不会引用任何 curl 符号）。Windows 与 macOS 自带该头文件。

## 开发

```bash
just bootstrap   # 检查工具链 + 安装前端依赖
just dev         # 启动桌面应用（Vite + Rust）
just web         # 仅启动前端，浏览器调试界面
just typecheck   # tsc --noEmit
just rust-check  # cargo check
just check       # CI 会跑的全部检查
just icons       # 由脚本重新生成全部图标
```

## 发布

发布由版本号驱动：把版本号改掉并推到 `main`，GitHub Actions 就会打 tag、为三个平台
打包安装包与免安装可执行文件，并生成 Release 说明。

```bash
# 先在 CHANGELOG.md 把 `## [Unreleased]` 改成 `## [0.2.0] - 2026-09-16`
just release-plan   # 预览：会不会发版、Release 说明的内容
just release 0.2.0  # 同步版本号 → just check → 提交 → 推送
```

完整流程、工作流行为与排错看 [AGENTS.md](./AGENTS.md)，版本变更记录看
[CHANGELOG.md](./CHANGELOG.md)。

## 本地测试环境

`docker-compose.yml` 里准备了三种 broker，全部映射到 localhost：

```bash
just all-up      # 全部启动
just rabbit-up   # RabbitMQ 4 + management（5672 / 15672，guest/guest）
just rocket-up   # RocketMQ 5.3 nameserver + broker（9876 / 10911）
just kafka-up    # 单节点 Kafka KRaft（9092）+ kafka-ui（8080）
just kafka-seed  # 造几个主题并写入示例消息
```

对应连接参数：

| Broker | 主机 | 端口 | 其他 |
| --- | --- | --- | --- |
| Kafka | localhost | 9092 | 无需认证 |
| RabbitMQ | localhost | 5672 | 用户名 / 密码 `guest`，vhost `/` |
| RocketMQ | localhost | 9876（nameserver） | broker 端口 10911 由路由信息返回 |

`just rabbit-down` / `rocket-down` 只停对应容器，`*-reset` 会连同数据卷一起删除。
RocketMQ 的 `brokerIP1` 配置在 `docker/rocketmq/broker.conf`，必须写成宿主机能访问的
地址（容器场景即 `127.0.0.1`），否则客户端拿到的是容器内网 IP。

## 目录结构

```
src/                      前端（React）
  api/                    Tauri 命令封装
  components/             UI 组件，domain/ 下是业务组件
  pages/                  路由页面
  store/                  zustand 状态（工作区、连接状态、能力表）
  styles/                 设计变量与组件样式
src-tauri/src/
  commands/               #[tauri::command] 入口，只依赖中立模型
  mq/types.rs             中立模型（主题、消息、消费组、能力、连接字段）
  mq/provider.rs          MqProvider / MqConnection 契约
  mq/registry.rs          驱动注册表
  mq/drivers/<broker>/    每个 broker 一个模块
  mq/drivers/codec.rs     payload 编解码（utf8 / base64）
scripts/                  图标生成、版本号、Release 说明、Kafka 造数、环境检查
docker/                   docker compose 需要的配置文件
.github/workflows/        CI 与三平台 Release 打包
```

## 新增一个驱动

1. 在 `src-tauri/src/mq/drivers/` 下新建目录，实现 `MqProvider`：
   `descriptor()` 声明 id / 名称 / 强调色 / 连接字段 / `Capabilities`，
   `connect()` 返回一个实现了 `MqConnection` 的类型。
2. 在 `mq/drivers/mod.rs` 的 `registry_from_builtin()` 里 `registry.register(...)`。
3. 前端无需改动：连接表单、能力标记、页面按钮都从描述符里推导。

命令层与界面只读取 `Capabilities`，例如 RabbitMQ 关闭了 `topics.partitions`，
主题表单就不会再显示分区数、主题列表也会隐藏分区列。

改动约定、提交与发版流程见 [AGENTS.md](./AGENTS.md)。
