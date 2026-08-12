# 设计文档：history-session-archive

## 概述

历史记忆作为 `tools/history/` 独立领域模块加入现有 MCP 工具内核。Markdown 档案保存不可替代的事实，`memory/` 保存可丢弃、可重建且有大小上限的派生视图。MCP 层只提取 ChatGPT `_meta["openai/session"]` 并传入历史工具；工具注册表暴露五个工具；历史模块负责解析、校验、锁、原子写入、脱敏、搜索和读取。既有工具分支、Schema 和结果不变。

**对应需求:** FR-1 至 FR-10，NFR-1 至 NFR-6

## 技术方案

| 类别 | 选择 | 理由 | 关联需求 |
|---|---|---|---|
| 服务协议 | 现有 MCP 2025-06-18 Streamable HTTP | ChatGPT 网页版已通过现有 `/mcp` 连接，无需 OpenAI SDK | FR-1, FR-8 |
| 事实存储 | 数字 Markdown `N.md` | 用户可审阅、Git 友好、逐字保留、无需运行时数据库 | FR-1, FR-3, NFR-6 |
| 派生状态 | `memory/state.json` 和 `memory/manifest.json` | 有界快速恢复和定位，不复制档案全文，可从档案重建 | FR-2, FR-4, FR-6 |
| 搜索 | Rust 文件扫描和确定性关键词评分 | 无外部依赖、无需 embedding、能直接匹配原始用户输入 | FR-4, FR-8 |
| 序列化 | `serde`、`serde_json` | 与现有工具结果一致 | FR-2, FR-6 |
| 内容哈希 | 现有 `sha2` | 提供读取一致性和审计，无新增依赖 | FR-4, FR-5 |
| 跨进程锁 | `fs2::FileExt` | Windows、macOS、Linux 统一文件锁 | FR-6, NFR-3 |
| 原子替换 | Unix `rename`；Windows `MoveFileExW` | 确保 state、manifest 和新档案不产生半写文件 | FR-6, NFR-3 |

明确不采用 SQLite、向量库、外部模型、外部检索服务或 MCP Resource 自动读取。其收益不足以抵消 ChatGPT 连接器部署、用户配置和故障面的增加。

## 架构设计

```text
ChatGPT tools/call
  -> mcp/server.rs 提取 params._meta["openai/session"]
  -> 仅为 history_session_* 注入内部 _host_session_key
  -> tools/dispatch.rs 唯一分发入口
  -> tools/history/mod.rs 用例编排
       -> storage.rs 路径、锁、扫描、派生文件、搜索、读取
       -> markdown.rs 档案解析、渲染、修订、脱敏
       -> model.rs 输入、档案、状态、manifest、搜索模型
  -> wrap_mcp_tool_result 保持既有 envelope
```

历史工具不接受客户端传入的任意 `workspace_root` 作为权限根。当前 MCP Runtime 的 `Workspace` 是唯一可信边界；兼容字段 `workspace_root` 存在时必须与已绑定工作区规范路径相同。

## 文件结构

```text
docs/history-session/
  1.md                         # 永久事实档案
  2.md
  ...
  memory/
    state.json                 # 当前状态的有界物化视图
    manifest.json              # 档案目录与定位元信息
```

### 事实边界

- `N.md` 是事实源。创建会话时写入首次用户输入；checkpoint 只向当前会话追加 turn 或 superseding revision。bootstrap、validate、manifest/state 重建永不改写既有 `N.md`。
- `state.json` 是用于 bootstrap 的有界投影，不保存每个历史文件的全文或递归摘要。损坏、删除或过期时可从档案确定性重建。
- `manifest.json` 只保存编号、路径、标题、创建/更新时间、字节数、SHA-256、关键词与少量定位信息；不复制正文。
- 旧版 `index.json` 是兼容迁移输入而非新事实源。实现可读取它辅助发现映射，但不依赖它正确性，也不会为重建目的修改档案。

## 数据模型

### 档案 Markdown

```markdown
# 会话 12：修复远程 MCP bootstrap

**Session key:** anonymous-chat-session
**Created:** 2026-08-09T10:30:00+08:00
**Updated:** 2026-08-09T10:33:00+08:00
**Status:** active

## 首次用户输入
逐字保存的首次请求。

## 用户输入记录
### turn-0001 revision-1
逐字保存的本轮用户输入。

## 本轮检查点
### turn-0001 revision-1
结构化交接字段。
```

首次输入和每轮输入由调用方显式提供。服务端不能读取未作为工具参数传来的 ChatGPT 转录；参数缺失必须在结果中明确。命中明确密钥模式时，原始值替换成 `[REDACTED]`，并以 warning 和档案元信息说明。除此以外不做内容压缩或改写。

### `memory/state.json`

```json
{
  "version": 2,
  "state_revision": 18,
  "archive_revision": "sha256:...",
  "generated_at": "2026-08-09T10:33:00+08:00",
  "current_session": { "number": 12, "path": "docs/history-session/12.md" },
  "current_focus": "有界状态的简短当前焦点",
  "recent_changes": ["最多有限条目的近期事实"],
  "open_items": ["最多有限条目的待处理事实"],
  "references": [{ "number": 9, "path": "docs/history-session/9.md", "reason": "相关决定" }]
}
```

字段按元素数和 UTF-8 字节数限制。被省略的历史事实不会丢失，始终可以通过 `history_session_search` 和 `history_session_read` 获取。`archive_revision` 是 manifest/档案快照哈希，用于说明 state 是否需要重建。

### `memory/manifest.json`

```json
{
  "version": 2,
  "archive_revision": "sha256:...",
  "entries": [{
    "number": 12,
    "path": "docs/history-session/12.md",
    "title": "修复远程 MCP bootstrap",
    "created_at": "2026-08-09T10:30:00+08:00",
    "updated_at": "2026-08-09T10:33:00+08:00",
    "bytes": 4821,
    "sha256": "...",
    "keywords": ["bootstrap", "mcp", "history"]
  }]
}
```

### 修订和幂等性

`turn_id` 加其已脱敏的完整归档内容哈希组成幂等键。相同 key 的重试不写入；相同 `turn_id` 不同哈希会追加 `revision-N`，新块记录 `supersedes: revision-(N-1)`。旧块永久保留，state 只投影最新 revision。

## API 设计

| MCP 工具 | 输入 | 输出 | 关联需求 |
|---|---|---|---|
| `history_session_bootstrap` | `session_key`、`title`、`initial_user_input`、`workspace_root`、`history_dir`、`create_if_missing` | 编号、路径、输入捕获状态、状态版本、档案版本、统计、有限当前状态、检索指引、warnings | FR-1, FR-2 |
| `history_session_checkpoint` | `session_key`、`turn_id`、`raw_user_input` 和结构化交接字段 | 路径、档案 hash、输入捕获状态、幂等/修订状态、warnings | FR-3, FR-7 |
| `history_session_search` | `query`、`limit`、`cursor`、可选会话过滤 | 有限命中项、片段、分数、统计、`next_cursor` | FR-4 |
| `history_session_read` | `number` 或安全相对路径、`cursor`、`max_bytes` | 原始 UTF-8 内容页、总字节、SHA-256、`next_cursor` | FR-5 |
| `history_session_validate` | `workspace_root`、`history_dir`、`repair` | 档案、state、manifest 完整性报告与可重建结果 | FR-6 |

会话身份优先级固定为 `_host_session_key`、显式 `session_key`、结构化错误。`_host_session_key` 不出现在公开 Schema 中。

### Bootstrap 响应上限

bootstrap 只返回 state 中的有限字段，`recent_changes`、`open_items`、`references` 和文本字段分别限制项目数量与总 UTF-8 字节数。响应不包含档案全文、不递归包含摘要、不复制 manifest 全量条目。序列化后工具结果的验收上限为 64 KiB；超过预算时保留版本、统计、当前会话和搜索指引，并报告 `state_truncated=true`。截断只作用于派生响应，不作用于档案。

### 搜索和读取

搜索将 query 按空白和标点切词，先匹配 manifest，再按需流式扫描数字 Markdown。标题、原始用户输入、结构化章节和近期档案有不同权重。结果按分数、更新时间、编号稳定排序，片段在 UTF-8 字符边界截取。

读取只允许纯数字 `N.md` 或 manifest 产生的相对路径。`max_bytes` 默认 `32 KiB`、最大 `64 KiB`；无论调用方是否显式传入，读取都在 UTF-8 字符边界分页。返回内容 hash 和总字节数，客户使用 `next_cursor` 续读并在续页前比较 hash；不匹配时返回可恢复错误，避免静默读取混合版本。页大小只约束单次远程传输，不截断、摘要或改变长期档案。

## 关键流程

### 首次 bootstrap

1. MCP 层提取宿主会话 key，并仅注入历史工具调用。
2. 用例取得目录锁，扫描数字 Markdown，读取或重建 manifest/state。
3. 若无映射且允许创建，创建最小新档案并逐字写入 `initial_user_input`；缺参则写入缺失标记和 warning。
4. 更新 manifest 和 state 的原子 JSON 文件。此前已有的 `N.md` 不变。
5. 返回有界 state、版本、统计和 search/read 指引。

### checkpoint

1. 取得目录锁并验证 session 已 bootstrap。
2. 对输入执行明确的敏感信息脱敏，记录 warning。
3. 比较 turn 内容哈希：相同则返回幂等结果；不同则追加 superseding revision。
4. 只写入当前会话档案，随后从档案重建或增量更新 manifest/state 并原子替换。

### 搜索和读取

1. 读取或重建 manifest，不向 bootstrap 传播正文。
2. 搜索根据有界请求页扫描候选，返回有限片段与定位信息。
3. 读取校验安全编号、路径、cursor 与 hash，返回原始 UTF-8 文本页。

### validate/repair

1. 扫描档案、检查编号缺口、空文件、非法文件、重复 session key 和派生 JSON 完整性。
2. `repair=false` 仅报告；`repair=true` 只重建 manifest/state 和兼容索引。
3. 无论结果如何都不更改已有数字 Markdown。

## 错误模型

历史模块使用现有 `WorkspaceError::ToolDetails` 返回 `code`、`message`、`category`、`retryable` 和 `details`。主要代码包括 `SESSION_ID_UNAVAILABLE`、`SESSION_NOT_BOOTSTRAPPED`、`HISTORY_SEQUENCE_CONFLICT`、`HISTORY_LOCK_FAILED`、`HISTORY_INDEX_CONFLICT`、`HISTORY_READ_NOT_FOUND`、`HISTORY_CURSOR_INVALID`、`PATH_OUTSIDE_WORKSPACE` 和 `HISTORY_WRITE_FAILED`。

## 设计决策

### 决策 1: Markdown 是永久事实源

不使用 SQLite 或数据库。用户已有的 `N.md` 是可审阅、可版本控制、跨平台且零配置的存储；派生 JSON 只承担性能职责。这样修复 bootstrap 大小问题时，不会牺牲首次输入或历史精度。

### 决策 2: 有界状态代替递归总结

不把各历史摘要合并写进最后一个档案，也不在最后一个摘要里再包含前序文件摘要。状态引用相关档案而非复制其内容，避免“最后一份摘要什么也没做却携带所有前文”的重复累积。

### 决策 3: 工具化按需恢复优于隐式 Resources

新版 MCP 支持 Resources，但 ChatGPT 连接器不会可靠地自动读取资源。search/read 是显式、可测、可分页的主路径；未来暴露 Resources 时仅提供便利，不改变此保证。

### 决策 4: 服务端只保存工具参数能证明的内容

远程 MCP 不能访问 ChatGPT 全部对话，必须让提示词和 Schema 明确传 `initial_user_input`/`raw_user_input`。缺失时公开报告，避免产生“看似已保存但实际丢失”的错误信任。

### 决策 5: 保留证据而非覆盖同 turn

网络重试必须幂等，但同一 `turn_id` 参数变化常意味着模型修订或用户补充。追加版本可以保持审计和精度，state 只使用最终有效版本以保持工作视图简洁。

### 决策 6: 不改变全局 MCP envelope

`wrap_mcp_tool_result` 位于 MCP 与 Actions 共用主链，改变它的风险高。先从根本消除历史工具的无界响应；不依赖修改该 envelope 解决重复序列化。未来若协议测试确认 ChatGPT 可读，可单独优化 envelope。

## 风险评估

| 风险 | 影响 | 缓解措施 |
|---|---|---|
| 派生 state 意外丢失 | 中 | 从 Markdown 无损重建；档案不依赖 state |
| 关键词检索不等于语义搜索 | 中 | 标题、用户输入和正文均可匹配；用户可按编号/路径无损读取 |
| Windows 覆盖式 rename 与 Unix 不同 | 高 | 使用 `MoveFileExW(MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH)` |
| 多个 ChatGPT 会话并发分配相同编号 | 高 | 目录级跨进程独占锁覆盖扫描、分配和派生文件写入全过程 |
| 模型输入包含密钥 | 高 | 明确模式脱敏、返回 warning、不静默改变内容 |
| 历史增长造成响应过大 | 高 | bootstrap 有严格预算，检索分页，正文仅由 read 显式返回 |
| 工具名注册与分发不一致 | 中 | Schema、tools/list、分发和 `_meta` 注入集成测试 |
| ChatGPT 继续使用升级前工具清单 | 中 | 不依赖版本号自动刷新；页面提示重新配置连接并新开会话 |
