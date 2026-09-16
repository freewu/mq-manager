# 更新日志

版本号写在 `src-tauri/tauri.conf.json`（由 `scripts/version.mjs` 同步到 `package.json`、
`Cargo.toml` 和 `Cargo.lock`）。发布时 `release` 工作流会取用与版本号同名的章节，再加上
本次推送的提交列表，作为 GitHub Release 的说明；因此**发版前必须在这里补一节**。

格式参照 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)。

## [Unreleased]

_暂无改动。_

## [0.2.0] - 2026-09-16

### 新增

- **RocketMQ 驱动**：nameserver + broker 集群视图、主题（读写队列，含系统主题标记）、
  订阅组与位点管理、生产消息、按队列非破坏性浏览与实时 tail。
  浏览使用独立订阅组 `CID_mq-manager-browse` 且从不提交位点，不会影响真实消费者。
- **RabbitMQ 驱动**：AMQP 0-9-1 负责连接与生产，Management API 完成队列、交换机、
  节点、消费者的枚举，以及 `ack_requeue_true` 的队首预览（peek）；tail 为轮询 + 指纹去重，
  不 ack 任何消息。
- 本地测试环境：`docker-compose.yml` 增加 RabbitMQ（含 management 插件）与 RocketMQ
  （nameserver + broker），配套 `just all-up` / `rabbit-up` / `rocket-up` / `*-logs` /
  `*-reset` 等命令与 `docker/rocketmq/broker.conf`。
- 发布流程：`scripts/version.mjs`、`scripts/release-plan.mjs` 与
  `.github/workflows/release.yml`，改版本号即自动打 tag 并打包三个平台。

### 变更

- 主题强调色改为 `#98d98e`；浅色强调色需要深色前景，按钮与勾选框改用
  `var(--text-inverse)`，品牌渐变与图标同步重新生成。
- 界面按驱动声明的 `Capabilities` 渲染：不支持分区的 broker 不再显示分区数、
  副本因子与分区列，浏览 / 位点 / purge 等按钮同样按能力开闭。
- MSRV 提升到 Rust 1.88（`lapin` 4.x 的要求），TLS 统一走 rustls + ring，
  不再需要 CMake / NASM 之外的本地依赖。
- `README.md` 重写：驱动能力对比、非破坏性浏览的实现方式、本地环境端口对照表，
  以及新增一个驱动需要改哪些文件。

## [0.1.0] - 2026-09-15

### 新增

- 首个版本：Tauri 2 + React + Vite 的通用消息队列客户端。
- Kafka 驱动（`rdkafka`）：主题 / 分区 / 消费组 / 位点管理，生产消息，
  非破坏性浏览与实时 tail。
- 驱动注册表与中立模型（`MqProvider` / `MqConnection` / `Capabilities`）：
  命令层与界面不认识任何具体 broker，新增一种消息队列只需注册一行。
- 连接配置管理、工作区状态、能力标记与 `just` 开发命令集。

[Unreleased]: https://github.com/freewu/mq-manager/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/freewu/mq-manager/releases/tag/v0.2.0
[0.1.0]: https://github.com/freewu/mq-manager/releases/tag/v0.1.0
