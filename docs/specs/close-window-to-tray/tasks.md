# 任务清单：close-window-to-tray

## 概述

实现关闭到托盘功能，并确保 Niri `Super+Q` 不再中断 Coding Tools MCP 的后台服务。

> **二元禁令（零容忍）**：本文件及后续实现的交付物不得以占位符代替真实内容。

---

## 交付物清单（Scope-lock）

- **预计新建文件数**: 4 个（3 个规格文档、1 个 Rust 模块）
- **预计修改文件数**: 7 个
- **预计新增/修改函数数**: 约 7 个
- **交付物逐项列举**:
  1. `docs/specs/close-window-to-tray/requirements.md`
  2. `docs/specs/close-window-to-tray/design.md`
  3. `docs/specs/close-window-to-tray/tasks.md`
  4. `src-tauri/src/desktop_lifecycle.rs`
  5. `src-tauri/src/lib.rs`
  6. `src-tauri/Cargo.toml`
  7. `src-tauri/Cargo.lock`
  8. `src-tauri/tauri.conf.json`
  9. `package.json`
  10. `package-lock.json`
  11. 本机 `0.1.34` release 二进制与维护安装状态

---

## 任务列表

### 阶段 1: 准备工作

- [x] 1.1 校验现有窗口生命周期与托盘依赖，完成符号级影响分析
  - **证据块**: `src-tauri/src/lib.rs:127-134` 当前只在 `UI_RECREATING` 时阻止 `RunEvent::ExitRequested`；`src-tauri/src/commands/ui_memory.rs:10-14` 用原子标记保护 WebView 重建；`src-tauri/Cargo.toml:18` 尚未启用 Tauri `tray-icon` feature。
  - **涉及文件**: 只读 `src-tauri/src/lib.rs`、`src-tauri/src/commands/ui_memory.rs`、`src-tauri/Cargo.toml`；每个文件均低于 500 行。
  - _需求: FR-1, FR-2, FR-3_ ｜ _设计: 技术方案、风险评估_

### 阶段 2: 核心实现

- [x] 2.1 实现托盘与关闭决策模块，只隐藏主窗口并保留显式退出
  - **证据块**: `src-tauri/src/commands/ui_memory.rs:115-120` 已采用 `unminimize → show` 的恢复顺序；`src-tauri/src/commands/ui_memory.rs:189` 随后调用 `set_focus()`，新模块复用同一顺序。
  - **涉及文件**: 新建 `src-tauri/src/desktop_lifecycle.rs`，预算 180 行，不超过 500 行。
  - _需求: FR-1, FR-2, FR-3_ ｜ _设计: API 设计、设计决策 1 与 2_
- [x] 2.2 接入 Tauri setup/window/run 事件并启用 tray feature
  - **证据块**: `src-tauri/src/lib.rs:65-75` 是当前 setup 入口；`src-tauri/src/lib.rs:124-135` 是 build/run 生命周期回调；托盘和关闭事件应在这两个边界接线。
  - **涉及文件**: 修改 `src-tauri/src/lib.rs` 约 15 行、`src-tauri/Cargo.toml` 1 行，Cargo 自动同步 `src-tauri/Cargo.lock`。
  - _需求: FR-1, FR-2, FR-3_ ｜ _设计: 架构设计_
- [x] 2.3 将项目版本从 0.1.33 同步递增到 0.1.34
  - **证据块**: `docs/project-context/how-to-develop.md:55-75` 要求功能交付前递增 patch，并同步五个版本源。
  - **涉及文件**: 修改 `package.json`、`package-lock.json`、`src-tauri/Cargo.toml`、`src-tauri/Cargo.lock`、`src-tauri/tauri.conf.json` 各 1-2 行。
  - _需求: FR-1, FR-2, FR-3_ ｜ _设计: 文件结构_

### 阶段 3: 集成测试

- [x] 3.1 对照验收标准运行静态检查、Rust 测试与生命周期回归测试
  - **证据块**: `docs/project-context/how-to-test.md:8-16` 定义 Rust 单元/集成测试与前端检查层级；本功能在 Rust 生命周期模块内补单测，并运行全量测试。
  - **涉及文件**: `src-tauri/src/desktop_lifecycle.rs` 内测试模块；不新建额外测试文件。
  - _需求: FR-1, FR-2, FR-3_ ｜ _设计: 测试策略_
- [ ] 3.2 提交、构建并在 Niri 实机验证关闭、托盘恢复和后台保活
  - **证据块**: `docs/project-context/how-to-develop.md:77-88` 要求从版本提交构建并校验已安装版本；本机桌面入口直接运行 `src-tauri/target/release/coding-tools-mcp-desktop`。
  - **涉及文件**: 不再修改源码；生成 release 二进制并更新本机维护安装提交记录。
  - _需求: FR-1, FR-2, FR-3_ ｜ _设计: 测试策略_

---

## 检查点

- [x] 阶段 1 完成后：GitNexus 影响分析已记录；修改前的符号级评估为 LOW，完整 diff 的入口级检测为 HIGH，已按高风险结果执行全量测试和逐行审查。
- [x] 阶段 2 完成后：所有版本源为 0.1.34，本次触及的 Rust 文件通过 `rustfmt --check`，且 `git diff --check` 通过。
- [ ] 阶段 3 完成后：静态检查、全量测试、release 构建、Niri 关闭到托盘与公网 MCP 探测均通过。

---

## 需求覆盖矩阵

| 需求 ID | 设计章节 | 任务编号 | 状态 |
|---------|----------|----------|------|
| FR-1 | 架构设计、设计决策 1 | 1.1, 2.1, 2.2, 3.1, 3.2 | 实现与自动测试完成，待实机验证 |
| FR-2 | API 设计、架构设计 | 1.1, 2.1, 2.2, 3.1, 3.2 | 实现与自动测试完成，待实机验证 |
| FR-3 | 架构设计、测试策略 | 1.1, 2.1, 2.2, 3.1, 3.2 | 实现与自动测试完成，待实机验证 |

---

## 文件变更清单

| 文件 | 操作 | 行数预算 | 说明 |
|------|------|----------|------|
| `docs/specs/close-window-to-tray/*.md` | 新建 | 约 330 行 | 需求、设计与任务规格 |
| `src-tauri/src/desktop_lifecycle.rs` | 新建 | 约 180 行 | 托盘、关闭/退出决策与单测 |
| `src-tauri/src/lib.rs` | 修改 | 约 15 行 | 生命周期接线 |
| `src-tauri/Cargo.toml` | 修改 | 2 行 | tray feature 与版本 |
| `src-tauri/Cargo.lock` | 修改 | 1 行 | 项目包版本 |
| `src-tauri/tauri.conf.json` | 修改 | 1 行 | 应用版本 |
| `package.json` | 修改 | 1 行 | npm 包版本 |
| `package-lock.json` | 修改 | 2 行 | npm 根版本 |

---

## 检查清单

- [x] 交付物清单已锁定
- [x] 每条任务标题具体且可验收
- [x] 每条任务含证据块
- [x] 每条任务标注文件与行数预算
- [x] 任务粒度可在单次提交内完成
- [x] 每条任务回链到 FR 与设计章节
- [x] 需求覆盖矩阵无遗漏
- [x] 阶段 3 包含逐条验收
- [x] 全文无模板占位符
