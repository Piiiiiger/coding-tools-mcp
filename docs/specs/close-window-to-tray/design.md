# 设计文档：close-window-to-tray

## 概述

本设计在 Tauri 桌面生命周期层增加一个独立的 `desktop_lifecycle` 模块。模块负责托盘创建、主窗口显示/隐藏和显式退出状态；`lib.rs` 只负责把 Tauri setup、window event 与 run event 接入该模块。后台 `AppState`、MCP、Actions 和 tunnel supervisor 不参与窗口隐藏流程。

**对应需求:** FR-1、FR-2、FR-3、NFR-1、NFR-2、NFR-3、NFR-4

---

## 技术方案

### 技术选型

| 类别 | 选择 | 理由 | 关联需求 |
|------|------|------|----------|
| 托盘 | Tauri 2 `tray::TrayIconBuilder` 与 `menu` API | 与应用生命周期、默认图标和跨平台事件模型直接集成 | FR-2, NFR-3 |
| 关闭拦截 | `Builder::on_window_event` + `CloseRequested::api.prevent_close()` | 在窗口真正销毁前阻断，适配 Niri 发出的标准关闭请求 | FR-1 |
| 生命周期状态 | `AtomicBool` 保存 tray-ready 与 explicit-exit | 热路径无阻塞，便于纯函数决策测试 | FR-1, FR-3, NFR-4 |
| 恢复窗口 | 托盘菜单；支持的平台附加左键事件；随后 `unminimize → show → set_focus` | Linux 的 Tauri tray click event 不发出，因此菜单是跨平台保证路径 | FR-2 |

### 架构设计

```text
Niri Super+Q / 标题栏关闭
          │
          ▼
WindowEvent::CloseRequested
          │
          ├─ tray ready && !explicit exit ─► prevent_close ─► hide(main)
          │                                         │
          │                                         └─ MCP / tunnel 保持运行
          └─ otherwise ───────────────────────────► 正常关闭

Tray “显示窗口” / 支持平台的 left click ─► unminimize ─► show ─► focus
Tray “退出” ─► explicit_exit=true ─► AppHandle::exit(0)
```

`RunEvent::ExitRequested` 的最终决策为：仅当 UI WebView 正在重建且没有显式退出时调用 `prevent_exit()`。这样保留 `0.1.30` 的后台保活修复，同时确保托盘退出始终有效。

---

## 数据模型

不涉及持久化数据。进程内状态如下：

| 状态 | 类型 | 约束 | 说明 |
|------|------|------|------|
| `TRAY_READY` | `AtomicBool` | 默认 `false` | 托盘成功创建后为 `true`；只有该状态允许关闭到托盘 |
| `EXPLICIT_EXIT_REQUESTED` | `AtomicBool` | 默认 `false` | 托盘“退出”触发后为 `true`，允许应用真正退出 |

---

## API 设计

| 方法/函数 | 签名 | 入参 | 出参 | 关联需求 |
|-----------|------|------|------|----------|
| `install_tray` | `fn install_tray(app: &mut tauri::App) -> tauri::Result<()>` | Tauri App | 创建托盘或返回错误 | FR-2 |
| `handle_window_event` | `fn handle_window_event(window: &Window, event: &WindowEvent)` | 窗口与事件 | 无 | FR-1 |
| `should_prevent_app_exit` | `fn should_prevent_app_exit(ui_recreating: bool) -> bool` | UI 重建状态 | 是否阻止进程退出 | FR-3 |
| `show_main_window` | `fn show_main_window(app: &AppHandle)` | AppHandle | 无 | FR-2 |

---

## 文件结构

```text
src-tauri/
├── src/
│   ├── desktop_lifecycle.rs  # 新增：托盘与关闭生命周期
│   └── lib.rs                # 修改：接入 setup/window/run 事件
├── Cargo.toml                # 修改：启用 tray-icon feature 与版本 0.1.34
├── Cargo.lock                # 修改：同步包版本
└── tauri.conf.json           # 修改：同步应用版本
package.json                  # 修改：同步版本
package-lock.json             # 修改：同步根包版本
```

---

## 设计决策

### 决策 1: 托盘不可用时允许正常关闭（关联需求: FR-1）

**问题**: 如果 Linux 桌面没有可用的 StatusNotifier 托盘，隐藏窗口后用户将无法恢复。

**选项**:

1. 无条件隐藏：行为一致，但可能产生不可恢复的后台进程。
2. 仅在托盘创建成功后隐藏：托盘不可用时退化为现有关闭行为。

**决策**: 选择仅在托盘创建成功后隐藏。

**理由**: 可恢复性优先于强制一致性，符合 NFR-2。

### 决策 2: 新建生命周期模块而不是继续堆叠 `lib.rs`（关联需求: FR-1, FR-2, FR-3）

**问题**: `lib.rs` 已包含应用装配与大量 command 注册，直接加入托盘事件会扩大耦合。

**选项**:

1. 全部内联在 `run()`。
2. 新建 `desktop_lifecycle.rs`，只在 `run()` 接线。

**决策**: 选择独立模块。

**理由**: 生命周期决策可独立单测，且不触碰 MCP/隧道模块。

---

## 测试策略

- Rust 单元测试覆盖关闭决策：托盘未就绪允许关闭、托盘就绪隐藏、显式退出允许关闭。
- Rust 单元测试覆盖退出决策：UI 重建阻止退出，显式退出覆盖重建状态。
- 运行 `npm run check`、`cargo check` 与完整 `cargo test`。
- Linux 实机验证：记录 MCP 公网与本地探测；触发主窗口关闭后确认进程、端口与隧道 PID 不变；托盘恢复窗口；托盘退出结束进程。
- 从提交后的 `0.1.34` 源码构建 release，再通过本机维护脚本记录并重启。

---

## 风险评估

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| Linux 桌面无托盘宿主 | 中 | 托盘初始化失败时不拦截关闭 |
| 显式退出被 UI 重建状态拦截 | 高 | 退出决策显式检查 `EXPLICIT_EXIT_REQUESTED`，补单测 |
| 关闭处理误伤 keepalive 窗口 | 中 | 只处理 label 为 `main` 的窗口 |
| WebView 重建后托盘恢复找不到窗口 | 低 | 每次从 `AppHandle` 动态查询当前 `main` 窗口 |

---

## 检查清单

- [x] 技术方案与现有 Tauri 架构一致
- [x] requirements.md 中每条 FR 均已覆盖
- [x] 文件结构基于真实代码路径
- [x] 状态与函数契约清晰
- [x] 关键设计决策已记录并关联需求
- [x] 测试策略可验证验收标准
