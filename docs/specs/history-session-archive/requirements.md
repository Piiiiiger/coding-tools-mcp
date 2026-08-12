# 需求文档：history-session-archive

## 功能概述

为 ChatGPT 网页版远程 MCP 开发增加跨会话、无损且可扩展的历史记忆。每个新聊天首次调用 `history_session_bootstrap` 时，服务端依据 `_meta["openai/session"]` 识别当前聊天，并在当前工作区的 `docs/history-session/` 保存或恢复会话。

数字 Markdown 档案 `N.md` 是永久事实源：首轮用户请求、每轮用户输入、检查点和修订证据都逐字保存，绝不因摘要、索引重建或 bootstrap 而覆盖、截断或删除。`memory/state.json` 是可从档案重建的有界当前状态；`memory/manifest.json` 是仅含定位元信息的轻量目录。bootstrap 只返回当前状态和检索指引，模型再使用搜索和读取工具按需恢复原文，因此历史增长不会使远程 MCP 初始化响应无界增长。

本功能复用现有 Streamable HTTP `/mcp` 和统一 `call_tool` 分发入口，不引入 SQLite、向量数据库、外部模型、OpenAI SDK 或额外用户配置。

## 历史经验与坑（来自记忆库）

- **可复用经验**: MCP 与 Actions 必须继续使用统一工具注册表和 `call_tool` 分发入口；工具名称、Schema、注册与分发由契约测试共同约束。
- **必须规避的坑**: 不允许注册名与分发名不一致；不在前后端重复触发生命周期；不把档案事实仅放在进程内缓存；不使用标题、首条消息或时间戳推测会话身份。
- **本次根因**: 旧 bootstrap 同时返回全量会话摘要、累计摘要、最新完整 handoff，且 MCP envelope 把结果重复序列化；约 40 个、3 MiB 的档案可产生约 3.5 MB 远程响应并接近超时或响应大小限制。

---

## 范围边界

- **In Scope**:
  - 新增或演进 `history_session_bootstrap`、`history_session_checkpoint`、`history_session_validate`、`history_session_search`、`history_session_read` 五个 MCP 工具。
  - 使用 ChatGPT `_meta["openai/session"]` 作为首选稳定会话标识，并允许显式 `session_key` 用于测试和兼容客户端。
  - 在 MCP Runtime 已绑定的工作区内读写 `docs/history-session/`。
  - 永久 Markdown 档案、可重建状态和 manifest、关键词搜索、分页、无损原文读取、跨进程文件锁、原子写入、结构化错误和敏感信息脱敏。
  - 支持 Windows、macOS、Linux 的路径、锁和原子替换语义。
- **Out of Scope**:
  - 修改或包裹现有编码工具的执行行为。
  - 读取 ChatGPT 完整聊天转录、浏览器 DOM 或调用 ChatGPT 私有接口。
  - 自动提交 Git、执行 Shell、删除历史档案或在工作区外写入。
  - SQLite、向量库、外部检索服务、外部模型和新的用户侧配置。
  - 依赖 MCP Resources 被 ChatGPT 自动读取；Resources 可在后续作为补充，工具调用仍是恢复主入口。

---

## 需求列表

### FR-1: 新会话恢复、编号与首次输入档案

**优先级:** Must

**用户故事:** 作为 ChatGPT 网页版开发者，我希望新聊天首次使用插件时自动建立或恢复会话，并完整保存我首次提出的请求，以便不必手工输入“恢复会话”且可追溯原始需求。

#### 验收标准（EARS）

1. WHEN ChatGPT 调用 `history_session_bootstrap` 且 `_meta["openai/session"]` 存在 THEN 系统 SHALL 使用该值作为当前 `session_key`，无需用户复制会话 ID。
2. WHEN 当前 `session_key` 首次出现 THEN 系统 SHALL 在锁内扫描历史、校验编号并创建下一个纯数字 Markdown 档案。
3. WHEN 同一 `session_key` 重复调用 THEN 系统 SHALL 返回原编号档案且不得重复创建。
4. WHEN `_meta["openai/session"]` 与显式 `session_key` 同时存在 THEN 系统 SHALL 使用宿主元数据并返回来源 `platform_conversation_id`。
5. IF 两种会话标识均不存在 THEN 系统 SHALL 返回 `SESSION_ID_UNAVAILABLE`，不得生成不可复用的临时 ID。
6. WHEN 首次创建会话且调用方提供 `initial_user_input` THEN 系统 SHALL 将该文本逐字写入该档案的首次用户输入记录，并返回 `initial_input_captured=true`。
7. WHEN 首次创建会话但调用方未提供 `initial_user_input` THEN 系统 SHALL 返回 `initial_input_captured=false` 和明确 warning，且不得宣称已捕获首轮输入。
8. WHEN bootstrap 重试且首次输入内容相同 THEN 系统 SHALL 保持幂等；WHEN 内容不同 THEN 系统 SHALL 追加带时间和来源的修订证据，不得覆盖旧文本。

### FR-2: 有界 bootstrap 与可重建当前状态

**优先级:** Must

**用户故事:** 作为远程 MCP 使用者，我希望初始化只携带足以开始工作的当前状态，而不是全部历史副本，以便历史会话很多时仍能可靠启动。

#### 验收标准（EARS）

1. WHEN bootstrap 成功 THEN 系统 SHALL 返回会话编号、相对路径、`state_revision`、`archive_revision`、历史统计、少量最近变化、检索指引和结构化 `assistant_instructions`。
2. WHEN bootstrap 成功 THEN 系统 SHALL NOT 返回 `all_history_summary`、`session_summaries`、`latest_handoff`、`inherited_summary` 或任何全量档案正文。
3. WHEN `memory/state.json` 存在且有效 THEN 系统 SHALL 读取其中的有界当前状态；WHEN 它缺失或损坏 THEN 系统 SHALL 从 Markdown 档案重建该状态。
4. WHEN 新会话创建 THEN 系统 SHALL NOT 将前序会话摘要或全文复制到新 `N.md`；跨会话上下文必须通过状态引用和按需读取获得。
5. WHEN 历史档案增长 THEN bootstrap 响应 SHALL 保持有界；40 个、约 3 MiB 档案的序列化工具结果目标小于 64 KiB。
6. WHEN 生成或重建状态 THEN 系统 SHALL 将其标记为派生视图和版本化索引，不得把它当作档案事实源。

### FR-3: 每轮原始用户输入与检查点修订证据

**优先级:** Must

**用户故事:** 作为持续开发的用户，我希望每轮用户请求和执行交接都能无损归档，并在网络重试或内容修订时仍保留证据，以便之后精确恢复。

#### 验收标准（EARS）

1. WHEN `history_session_checkpoint` 收到新的 `session_key + turn_id` THEN 系统 SHALL 将结构化检查点和 `raw_user_input` 逐字写入当前编号档案。
2. WHEN 调用方没有传入 `raw_user_input` THEN 系统 SHALL 返回 `user_input_captured=false` 与明确 warning，不得假装服务端可以读取 ChatGPT 未传递的聊天内容。
3. WHEN 相同 `turn_id` 以相同归档内容重试 THEN 系统 SHALL 不重复追加并返回 `duplicate_ignored=true`。
4. WHEN 相同 `turn_id` 的归档内容变化 THEN 系统 SHALL 追加 superseding 修订并保留旧版本、修订时间和替代关系；`state.json` SHALL 仅投影该 turn 的最新有效版本。
5. WHEN 写入成功 THEN 系统 SHALL 返回最终档案 SHA-256、会话编号、相对路径、修订信息和输入捕获状态。
6. WHEN 状态或 manifest 写入失败 THEN 系统 SHALL 保持已成功写入的档案可读，并返回可恢复的结构化错误或 warning；不得回写、截断或删除既有档案。

### FR-4: 按需历史搜索

**优先级:** Must

**用户故事:** 作为恢复会话的开发者，我希望根据任务名、决定或原始用户输入查找相关历史，避免把不相关历史塞进 bootstrap。

#### 验收标准（EARS）

1. WHEN 客户端调用 `history_session_search` 并提供 `query` THEN 系统 SHALL 对 manifest 标题、关键词和档案内容进行大小写无关的确定性关键词匹配和排序。
2. WHEN 匹配原始用户输入、标题或近期档案 THEN 系统 SHALL 给予更高排序权重，并返回编号、相对路径、hash、分数、标题、时间和有限 UTF-8 片段。
3. WHEN `query` 为空 THEN 系统 SHALL 按最近更新时间返回 manifest 条目，不扫描或返回全量正文。
4. WHEN `limit` 或 `cursor` 被提供 THEN 系统 SHALL 严格分页，返回稳定的 `next_cursor` 或明确的结束状态。
5. WHEN 没有匹配 THEN 系统 SHALL 返回空结果和统计信息，不将全部历史作为回退响应。

### FR-5: 无损历史读取

**优先级:** Must

**用户故事:** 作为需要精确上下文的开发者，我希望按编号或安全路径读取原始 Markdown，以便获取摘要无法表达的所有细节。

#### 验收标准（EARS）

1. WHEN 客户端以档案编号或 manifest 返回的安全相对路径调用 `history_session_read` THEN 系统 SHALL 返回对应 Markdown 的原始 UTF-8 文本。
2. WHEN `cursor` 或 `max_bytes` 被提供 THEN 系统 SHALL 在 UTF-8 字符边界分页，返回连续内容、总字节数、内容 SHA-256 和 `next_cursor`。
3. WHEN 未提供 `max_bytes` THEN 系统 SHALL 使用 `32 KiB` 默认页；WHEN `max_bytes` 超过 `64 KiB` THEN 系统 SHALL 拒绝该请求。该传输窗口不得截断、摘要或覆盖档案；客户端必须使用 `next_cursor` 精确恢复完整正文。
4. IF 编号不存在、路径不在 `docs/history-session/` 内、路径不是纯数字 Markdown 或 cursor 无效 THEN 系统 SHALL 返回结构化错误，不得读取工作区其他文件。
5. WHEN 多页读取同一档案 THEN 客户端 SHALL 能通过 hash 和 cursor 检测档案是否变化；系统不得静默拼接、摘要或改变原文。

### FR-6: 历史完整性、可重建索引与安全修复

**优先级:** Must

**用户故事:** 作为维护者，我想校验和恢复派生索引，以便发现档案问题而不丢失历史。

#### 验收标准（EARS）

1. WHEN `history_session_validate` 运行 THEN 系统 SHALL 报告编号、缺失编号、非法文件、空文件、重复 session key、最新编号、档案字节数、state 与 manifest 状态。
2. WHEN 发现 `1.md`、`3.md` THEN 系统 SHALL 报告缺失编号 2 且不得创建、覆盖或重编号 `2.md`。
3. WHEN `memory/state.json`、`memory/manifest.json` 或兼容索引缺失或损坏且 `repair=true` THEN 系统 SHALL 从 Markdown 元数据和正文重建派生文件。
4. WHEN `repair=false` THEN 系统 SHALL 保持只读。
5. WHEN bootstrap、validate 或派生状态重建运行 THEN 系统 SHALL NOT 修改任何既有 `N.md`；唯一允许写入档案的路径是创建新会话或对当前会话显式 checkpoint。
6. IF 发现无法确定的冲突 THEN 系统 SHALL 返回结构化告警，不得删除、改名或覆盖历史档案。

### FR-7: 跨平台文件安全与敏感信息边界

**优先级:** Must

**用户故事:** 作为跨平台开发者，我想安全地保存高保真历史，同时不会把明确识别出的凭据明文写进仓库。

#### 验收标准（EARS）

1. WHEN 解析 `history_dir` 或 read 路径 THEN 系统 SHALL 使用平台路径 API 并限制最终路径位于当前工作区的历史目录内。
2. IF 路径为绝对路径、包含父级穿越或经符号链接逃逸 THEN 系统 SHALL 返回 `PATH_OUTSIDE_WORKSPACE`。
3. WHEN 写入档案、state 或 manifest THEN 系统 SHALL 获取跨进程独占锁并使用同目录临时文件加原子替换。
4. IF 无法获得锁或原子替换失败 THEN 系统 SHALL 返回可恢复的结构化错误，不得留下半写文件。
5. WHEN 输入包含明确匹配的 API Key、Token、Cookie、Bearer、密码或私钥 THEN 系统 SHALL 将匹配值替换为 `[REDACTED]`，在 `warnings` 明示脱敏，并将该行为记录到档案元信息。
6. WHEN 内容不命中明确密钥模式 THEN 系统 SHALL 逐字保持用户输入和结构化字段；除安全脱敏外不得静默摘要、重写或截断档案正文。

### FR-8: 增量集成和工具契约

**优先级:** Must

**用户故事:** 作为现有工具用户，我想获得历史能力，同时保持既有工具行为不变。

#### 验收标准（EARS）

1. WHEN 客户端调用 `tools/list` THEN 系统 SHALL 在现有工具之外返回五个历史工具及完整 JSON Schema。
2. WHEN 调用历史工具 THEN MCP 与 Actions SHALL 继续通过唯一 `call_tool` 入口执行。
3. WHEN 调用任一既有工具 THEN 系统 SHALL 保持原输入、输出、权限和执行路径不变。
4. WHEN 构建依赖解析 THEN 系统 SHALL 不包含 OpenAI SDK、SQLite 驱动、向量数据库客户端或外部检索服务。
5. WHEN MCP 结果 envelope 序列化 THEN 新历史工具 SHALL 通过有界 bootstrap、分页搜索和显式读取控制负载；不得为兼容旧字段重新注入全量历史正文。

### FR-9: ChatGPT 持久化工作流提示

**优先级:** Must

**用户故事:** 作为 ChatGPT 网页版用户，我希望插件清楚说明何时恢复和保存当前轮输入，以便高精度状态不会在新聊天后消失。

#### 验收标准（EARS）

1. WHEN ChatGPT 初始化 MCP 连接 THEN 系统 SHALL 在 `initialize.result.instructions` 中说明：每个新聊天首次回复前调用 bootstrap 并传入逐字 `initial_user_input`；每轮任务完成、最终回复前调用 checkpoint 并传入逐字 `raw_user_input`。
2. WHEN bootstrap 成功 THEN 系统 SHALL 返回 `assistant_instructions`、`required_next_actions`、`checkpoint_policy`、输入捕获状态和历史检索指引。
3. WHEN 客户端调用 `tools/list` THEN bootstrap 和 checkpoint 描述 SHALL 明确各自的原始用户输入参数与服务端无法自行读取聊天转录的边界；search/read 描述 SHALL 明确它们是按需恢复入口。
4. WHEN 模型未执行 checkpoint 或未传入原始输入 THEN 服务端 SHALL 不宣称已自动持久化；第一版不修改或拦截现有工具，也不具备脱离模型调用的后台自动写入能力。
5. WHEN 用户打开任一工作区的 MCP 配置 THEN 页面 SHALL 展示完整会话恢复提示词和一键复制按钮，并提供复制成功或失败反馈。

### FR-10: ChatGPT 工具目录升级提示

**优先级:** Should

**用户故事:** 作为插件用户，我希望升级服务端后能明确知道如何让 ChatGPT 重新读取工具 Schema，以便不再误以为修改版本号会自动刷新。

#### 验收标准（EARS）

1. WHEN MCP 服务端版本升级但工具通知通道未实现 THEN 系统 SHALL NOT 声明 `capabilities.tools.listChanged=true`。
2. WHEN 用户查看工作区的 ChatGPT 会话提示区域 THEN 页面 SHALL 明确说明 ChatGPT 不会依据服务端版本号自动刷新工具。
3. WHEN 用户需要加载新工具目录 THEN 页面 SHALL 提供 ChatGPT 连接器设置入口，并要求重新配置连接后新开会话。
4. WHEN 未来实现 `notifications/tools/list_changed` 的可达通知通道 THEN 系统 MAY 将 `listChanged` 改为 `true`，但必须有协议级集成测试证明通知能够到达客户端。

---

## 非功能需求

- **NFR-1（兼容性）**: 支持 Windows 10+、macOS 12+、Linux x86_64；相同测试向量在三平台产生等价 JSON 结果。
- **NFR-2（安全）**: 所有历史路径限定在当前工作区；不执行 Shell、不删除档案、不自动提交 Git；服务器端不信任模型输入。
- **NFR-3（一致性）**: 同一进程和跨进程并发 bootstrap 不得分配重复编号；写入失败后原档案保持完整。
- **NFR-4（性能）**: 100 个、总计不超过 10 MiB 的档案 bootstrap 在本地 SSD 上应在 2 秒内完成，序列化结果保持小于 64 KiB；搜索默认页应在本地 SSD 上 2 秒内返回；单次历史正文读取默认不超过 32 KiB，最大不超过 64 KiB。
- **NFR-5（可维护性）**: 历史模块按模型、Markdown、存储和用例拆分；新增核心逻辑具备聚焦的单元和契约测试。
- **NFR-6（保真性）**: 除明确并可报告的密钥脱敏外，首次输入、每轮输入和档案正文不得因摘要、状态重建、迁移或分页丢失内容；派生状态丢失时必须可从档案重建。

---

## 依赖关系

- [OpenAI Apps SDK 官方变更记录](https://developers.openai.com/apps-sdk/changelog)（2026-01-15）：ChatGPT 工具调用会携带 `_meta["openai/session"]`，该匿名 conversation id 可用于关联同一 ChatGPT 会话内的请求。
- [MCP 规范](https://modelcontextprotocol.io/specification/2025-06-18)：工具调用结果允许结构化内容；客户端对 MCP Resources 的自动读取不构成可靠恢复契约。
- [OpenAI tunnel-client 连接器文档](https://github.com/openai/tunnel-client/blob/master/docs/connectors.md)：连接器侧 MCP 端点为 POST JSON-RPC；通知只会在进行中的流式请求里回传，普通无状态 JSON 响应不构成长连接通知通道。
- 现有 `src-tauri/src/mcp/server.rs`：读取 MCP `tools/call` 的 `_meta`。
- 现有 `src-tauri/src/tools/registry.rs`：注册工具及 Schema。
- 现有 `src-tauri/src/tools/dispatch.rs`：唯一执行入口。
- 现有 `Workspace`：工作区路径边界。
