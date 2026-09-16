# AGENTS.md

给在本仓库工作的 AI 助手（以及人类协作者）的约定。`README.md` 说明项目**是什么**，
这里说明改动后**该做什么**。

---

## 1. 每次开发完成：先验证，再提交并推送

**每完成一项开发任务，都要 `git commit` 并 `git push origin main`**，不要留下未推送的
提交，也不要让工作区里散落未提交的改动（用户明确要求时可例外，并在回复里说明）。

```bash
just check                       # 前端类型检查 + Rust 检查 + 前端测试 + Rust 测试
git add -A
git commit -m "feat(rocketmq): add consumer group offset reset"
git push origin main
```

- 提交信息用约定式提交，英文祈使句，范围用模块名：
  `feat(rabbitmq): …`、`fix(rocketmq): …`、`feat(ui): …`、`docs: …`、`chore(release): v0.2.0`。
- 没跑通 `just check` 就不要提交。只改了前端可以只跑 `just typecheck`，
  只改了后端可以只跑 `just rust-test`，但推送前至少把受影响的一侧跑完。
- `just check` 会执行 `pnpm test`（vitest）与 `cargo test`；
  Rust 侧要求 **零 warning**，不要用 `#[allow]` 掩盖真正的问题。
- 不要提交 `node_modules/`、`dist/`、`src-tauri/target/`、`.cargo/config.toml`
  （均为本机产物，已在 `.gitignore` 里）。

## 2. 发版：改版本号即触发 GitHub Actions 三平台打包

发布完全由版本号驱动，不需要手动打 tag：

```bash
# 1. 把 CHANGELOG.md 的 `## [Unreleased]` 改成 `## [0.2.0] - YYYY-MM-DD`，
#    并补上这一版的变更摘要（它就是 Release 说明的来源）。
# 2. 同步版本号（tauri.conf.json / package.json / Cargo.toml / Cargo.lock）
just set-version 0.2.0          # 等价于 node scripts/version.mjs 0.2.0
just release-plan               # 预览：会不会发版、Release 说明长什么样
# 3. 提交并推送（just release 会把上面三步串起来）
just release 0.2.0              # 或 git add -A && git commit -m "chore(release): v0.2.0" && git push origin main
```

推送后 `.github/workflows/release.yml` 会自动执行：

1. **plan** — `scripts/release-plan.mjs` 判断是否真的需要发版，并生成 Release 说明。
2. **verify** — `pnpm typecheck` / `pnpm test` / `cargo test`，不通过就不产出安装包。
3. **build** — 三个平台并行打包，并**先创建草稿 Release**，把产物传上去：

   | 平台 | 运行器 | 产物 |
   | --- | --- | --- |
   | Windows x64 | `windows-latest` | NSIS 安装包（`*-setup.exe`）+ 免安装 `mq-manager.exe` |
   | macOS（Intel + Apple Silicon） | `macos-latest` | 通用 `dmg` + 免安装 `mq-manager` |
   | Linux x64 | `ubuntu-22.04` | `deb`、`AppImage` + 免安装 `mq-manager` |

4. **publish** — 所有平台都成功后，把草稿 Release 转正（`gh release edit --draft=false`）。
   中途失败则停留在草稿状态，不会对外发布一个残缺版本。

行为细节（改动工作流时请保持）：

- 只有 **`src-tauri/tauri.conf.json` 里的版本号发生变化**才会触发；同一版本号重复推送、
  或者 `v<version>` 标签已存在，都会跳过（`release-plan.mjs` 负责判断）。
- 需要补发/重跑时用 `workflow_dispatch`；也可以删掉旧 tag 与 Release 后重跑。
- `uploadPlainBinary: true` 是三平台「可单独运行的可执行文件」的来源，
  不要关掉；`uploadUpdaterJson` 关闭是因为项目没有配置自动更新。
- 仓库需要 `Settings → Actions → General → Workflow permissions` 允许
  **Read and write permissions**，否则创建 Release 会 403。
- Release 说明 = `CHANGELOG.md` 中对应版本章节（缺失时回退到 `[Unreleased]`）
  \+ 本次推送的提交列表。所以**发版前必须写 CHANGELOG**，否则说明里只有提交列表。
- **librdkafka 的 `<curl/curl.h>` 坑**：`rdkafka_conf.c` 把该 include 写在
  `#ifdef WITH_OAUTHBEARER_OIDC` 里，而 cmake 用 `#cmakedefine01` 生成这个宏
  （即使功能关闭也会被"定义"成 0），于是任何平台编译 `rdkafka-sys` 都要求存在这个头文件，
  即使一个 curl 符号也不会被引用。CI 里因此先用 `CFLAGS=-I<空占位目录>` 骗过预处理器
  （见 `release.yml` 的 "Provide a curl header" 步骤）；本地 Linux 直接
  `sudo apt install libcurl4-openssl-dev` 即可（Ubuntu 把它放到
  `/usr/include/x86_64-linux-gnu/curl/`，属于 gcc 默认搜索路径）。
  升级 `rdkafka` 之后记得重新确认这个坑还在不在。

## 3. 硬性约束

- **MSRV 是 Rust 1.88**（`lapin` 4.x 的要求）。提升它之前先确认所有依赖都能在
  Windows / macOS / Linux 的 CI 上编译。
- **TLS 只允许 rustls + `ring`**：本机与 CI 都没有 aws-lc-rs 需要的 cmake/nasm/perl
  组合，新依赖必须用 `rustls--ring` / `rustls-no-provider` 这类 feature，
  不要引入 `aws-lc-rs`、`openssl`。
- **新增 broker = 新增一个驱动模块 + 注册一行**：实现 `MqProvider` / `MqConnection`，
  在 `src-tauri/src/mq/drivers/mod.rs::registry_from_builtin()` 里注册。
  命令层与界面不能出现具体 broker 的名字，能力差异一律通过 `Capabilities` 表达。
- **浏览 / tail 绝不能消费消息**：Kafka 用独立 group + `assign`，
  RabbitMQ 用 `ack_requeue_true` 的 peek，RocketMQ 用从不提交位点的临时订阅组。
- 本地测试环境用 `docker-compose.yml` + `just kafka-up` / `rabbit-up` / `rocket-up`；
  新增 broker 时补齐对应的 compose 服务、`just` 命令和 README 端口表。
- 中文文档、英文代码注释与提交信息（与现有代码保持一致）。

## 4. 提交前自检清单

- [ ] `just check` 通过（Rust 侧零 warning）。
- [ ] 改动涉及界面时，`pnpm typecheck` 通过，并确认能力差异走的还是 `Capabilities`。
- [ ] 新驱动/新功能已在 `README.md` 与 `CHANGELOG.md` 的 `[Unreleased]` 里写明。
- [ ] 已 `git push origin main`。
- [ ] 若是发版：版本号四处同步、CHANGELOG 有对应章节、Release 说明已在
      `just release-plan` 里预览过。
