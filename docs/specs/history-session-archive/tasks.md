# 任务清单：history-session-archive

## 概述

将已上线的会话归档从“bootstrap 携带全量历史”演进为“永久原文档案 + 有界状态 + 按需检索/读取”。实现保持既有工具行为不变，不引入 SQLite、向量库、外部模型或额外配置。

## 交付物清单（Scope-lock）

- 预计修改实现文件数: 9
- 预计修改测试与文档文件数: 5
- 预计任务数: 9

## 任务列表

### 阶段 1: 契约与规格

- [x] 1.1 确认远程 bootstrap 失败根因和边界
  - 证据: 40 个、约 3 MiB 档案产生约 3.5 MB MCP 响应；`structuredContent` 与 `content.text` 重复承载结果。
  - 需求: FR-2, NFR-4, NFR-6
- [x] 1.2 完成公共历史入口与 MCP 分发影响分析
  - 证据: `bootstrap`、`checkpoint`、`call_tool` 和 MCP 返回 envelope 属于 HIGH/CRITICAL 公共链路。
  - 需求: FR-8, FR-9
- [x] 1.3 更新需求、设计和任务规格
  - 文件: `docs/specs/history-session-archive/requirements.md`、`design.md`、`tasks.md`
  - 证据块: 已确认的无损档案、64 KiB 有界 bootstrap、按需 search/read 决策。
  - 需求: FR-1 至 FR-10, NFR-1 至 NFR-6
- [ ] 1.4 执行规格完整性校验并根据报告修正
  - 证据块: `mcp__mcp_probe_kit__check_spec` 返回的 requirements/design/tasks 完整性报告。
  - 验收: 无占位、每个 FR 有验收标准并进入覆盖矩阵。
  - 需求: FR-1 至 FR-10

### 阶段 2: 无损档案与派生状态

- [ ] 2.1 扩展历史模型
  - 文件: `src-tauri/src/tools/history/model.rs`
  - 证据块: `src-tauri/src/tools/history/model.rs` 现有索引、会话和 checkpoint 数据结构。
  - 实现: `MemoryState`、`MemoryManifest`、manifest 条目、搜索命中、输入捕获和 revision 模型。
  - 需求: FR-1, FR-2, FR-3, FR-4, FR-5
- [ ] 2.2 演进 Markdown 解析和渲染
  - 文件: `src-tauri/src/tools/history/markdown.rs`
  - 证据块: `src-tauri/src/tools/history/markdown.rs` 的档案渲染、解析和 turn 更新逻辑。
  - 实现: 首次输入、每轮原始输入、幂等 hash、superseding revision；消除 inherited summary 复制。
  - 需求: FR-1, FR-2, FR-3, FR-7, NFR-6
- [ ] 2.3 演进存储层
  - 文件: `src-tauri/src/tools/history/storage.rs`
  - 证据块: `src-tauri/src/tools/history/storage.rs` 的目录锁、扫描、索引和原子写入逻辑。
  - 实现: `memory/` 路径、state/manifest 原子写入和重建、确定性关键词搜索、安全 UTF-8 分页读取（默认 32 KiB、最大 64 KiB 单页传输窗口）。
  - 需求: FR-2, FR-4, FR-5, FR-6, FR-7
- [ ] 2.4 编排 bootstrap、checkpoint、validate、search、read
  - 文件: `src-tauri/src/tools/history/mod.rs`
  - 证据块: `src-tauri/src/tools/history/mod.rs` 的 `bootstrap`、`checkpoint` 和 `validate` 公共用例。
  - 实现: 64 KiB bootstrap 预算、输入捕获 warning、修订投影、仅修复派生文件。
  - 需求: FR-1 至 FR-7, NFR-3, NFR-4, NFR-6

### 阶段 3: MCP 契约与提示词

- [ ] 3.1 注册并分发五个历史工具
  - 文件: `src-tauri/src/tools/registry.rs`、`src-tauri/src/tools/dispatch.rs`
  - 证据块: `src-tauri/src/tools/registry.rs` 的工具 Schema 与 `src-tauri/src/tools/dispatch.rs` 的唯一分发分支。
  - 实现: JSON Schema、`history_session_search`、`history_session_read` 分支；既有工具分支不变。
  - 需求: FR-4, FR-5, FR-8
- [ ] 3.2 更新 ChatGPT 会话元数据与初始化指令
  - 文件: `src-tauri/src/mcp/server.rs`
  - 证据块: `src-tauri/src/mcp/server.rs` 对 `_meta["openai/session"]` 的提取和 initialize instructions。
  - 实现: 仅向五个历史工具注入宿主会话 key；指令要求传递首次和每轮原始用户输入。
  - 需求: FR-1, FR-3, FR-8, FR-9
- [ ] 3.3 更新工作区会话提示词
  - 文件: `src/lib/components/ChatGptSessionPrompt.svelte`
  - 证据块: `src/lib/components/ChatGptSessionPrompt.svelte` 的复制提示词模板。
  - 实现: 引导 bootstrap 传 `initial_user_input`、checkpoint 传 `raw_user_input`，通过 search/read 恢复档案。
  - 需求: FR-9, FR-10

### 阶段 4: 测试与真实档案验证

- [ ] 4.1 重写历史模块与 MCP 契约测试
  - 文件: `src-tauri/tests/history_session.rs` 及历史模块单元测试。
  - 证据块: `src-tauri/tests/history_session.rs` 的端到端历史工具调用与历史模块现有单元测试。
  - 验收: 有界 bootstrap、无损初始/每轮输入、修订证据、search/read、state 重建、路径防护、工具注册和 `_meta` 注入。
  - 需求: FR-1 至 FR-9, NFR-1 至 NFR-6
- [ ] 4.2 用真实 40 会话副本做完整性和性能验证
  - 输入: `E:\workspace\test\alphaloop_share\docs\history-session` 的副本。
  - 证据块: 副本内数字 Markdown 的 SHA-256 清单与 bootstrap 序列化响应字节数。
  - 验收: bootstrap、validate、派生文件重建前后旧 `N.md` SHA-256 完全一致；bootstrap 序列化结果小于 64 KiB；search/read 可定位并无损读取历史原文。
  - 需求: FR-2, FR-4, FR-5, FR-6, NFR-4, NFR-6
- [ ] 4.3 格式化、静态检查和影响范围复核
  - 证据块: `cargo check`、`cargo test`、`cargo clippy` 与 GitNexus `detect-changes` 输出。
  - 命令: `rtk cargo fmt`、`rtk cargo check`、`rtk cargo test`、`rtk cargo clippy --all-targets -- -D warnings`、`rtk npx gitnexus detect-changes --repo "E:\workspace\github\coding-tools-mcp-rust"`。
  - 需求: FR-8, NFR-1, NFR-3, NFR-5

## 需求覆盖矩阵

| 需求 ID | 设计章节 | 任务编号 | 状态 |
|---|---|---|---|
| FR-1 | 数据模型、首次 bootstrap、决策 4 | 1.3, 2.1, 2.2, 2.4, 3.2, 4.1 | 进行中 |
| FR-2 | 文件布局、Bootstrap 响应上限、决策 2 | 1.1, 1.3, 2.1, 2.3, 2.4, 4.2 | 进行中 |
| FR-3 | 修订和幂等性、checkpoint、决策 5 | 1.3, 2.1, 2.2, 2.4, 3.2, 4.1 | 进行中 |
| FR-4 | 搜索和读取、关键流程 | 1.3, 2.1, 2.3, 2.4, 3.1, 4.1, 4.2 | 进行中 |
| FR-5 | 搜索和读取、错误模型 | 1.3, 2.1, 2.3, 2.4, 3.1, 4.1, 4.2 | 进行中 |
| FR-6 | 文件布局、validate/repair、错误模型 | 1.3, 2.3, 2.4, 4.1, 4.2 | 进行中 |
| FR-7 | 数据模型、checkpoint、风险评估 | 1.3, 2.2, 2.3, 2.4, 4.1 | 进行中 |
| FR-8 | 架构设计、API 设计、决策 6 | 1.2, 1.3, 3.1, 3.2, 4.1, 4.3 | 进行中 |
| FR-9 | 决策 4、关键流程 | 1.2, 1.3, 3.2, 3.3, 4.1 | 进行中 |
| FR-10 | 风险评估 | 1.3, 3.3, 4.1 | 进行中 |
| NFR-1 | 技术方案 | 1.3, 4.1, 4.3 | 进行中 |
| NFR-2 | 架构设计、错误模型 | 1.3, 2.3, 4.1 | 进行中 |
| NFR-3 | 关键流程、风险评估 | 1.3, 2.3, 2.4, 4.1, 4.3 | 进行中 |
| NFR-4 | Bootstrap 响应上限 | 1.1, 1.3, 2.4, 4.2 | 进行中 |
| NFR-5 | 技术方案 | 1.3, 2.1, 2.2, 2.3, 2.4, 4.1, 4.3 | 进行中 |
| NFR-6 | 文件布局、决策 1、决策 5 | 1.1, 1.3, 2.2, 2.4, 4.1, 4.2 | 进行中 |

## 文件变更清单

| 文件 | 操作 | 说明 |
|---|---|---|
| `docs/specs/history-session-archive/requirements.md` | 修改 | 有界恢复、无损输入与 search/read 契约 |
| `docs/specs/history-session-archive/design.md` | 修改 | 事实档案、派生状态、检索和读取设计 |
| `docs/specs/history-session-archive/tasks.md` | 修改 | 实现顺序、验证证据与覆盖矩阵 |
| `src-tauri/src/tools/history/model.rs` | 修改 | state、manifest、搜索、输入与 revision 模型 |
| `src-tauri/src/tools/history/markdown.rs` | 修改 | 无损档案记录与修订渲染/解析 |
| `src-tauri/src/tools/history/storage.rs` | 修改 | 派生状态、搜索、读取、原子写入 |
| `src-tauri/src/tools/history/mod.rs` | 修改 | 五个历史用例和有界 bootstrap |
| `src-tauri/src/tools/registry.rs` | 修改 | 五个工具的 JSON Schema |
| `src-tauri/src/tools/dispatch.rs` | 修改 | search/read 唯一分发分支 |
| `src-tauri/src/mcp/server.rs` | 修改 | 会话元数据与输入持久化提示 |
| `src-tauri/tests/history_session.rs` | 修改 | 历史工具和 MCP 契约回归测试 |
| `src/lib/components/ChatGptSessionPrompt.svelte` | 修改 | ChatGPT 使用提示词 |

## 交付前自检

- [ ] bootstrap 不含全量历史、递归摘要或完整 handoff，序列化响应小于 64 KiB。
- [ ] 首次输入和每轮输入均从调用参数逐字保存；缺失参数和脱敏均有公开 warning。
- [ ] 相同 turn 同内容幂等，不同内容保留 superseding revision 证据。
- [ ] state/manifest 可重建，重建和 validate 不修改旧数字 Markdown。
- [ ] search/read 可以精确定位并通过默认 32 KiB、最大 64 KiB 的分页无损读取单个档案。
- [ ] 未引入 SQLite、向量库、外部模型或额外用户配置，既有工具无回归。
