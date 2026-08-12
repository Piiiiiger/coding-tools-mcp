# 设计文档：close-to-tray

## 概述

对应需求 FR-1～FR-5：关窗时用应用内三按钮确认替代直接退出；「后台运行」隐藏到托盘并保持 MCP/Actions/隧道；「直接关闭」与托盘「退出」真退出；与 `recreate_ui_webview` 隐藏态协同；Windows 二次启动尽量唤起已有窗口。

## 技术方案

### 关闭拦截（前端优先）

主窗使用 `@tauri-apps/api/window` 的 `onCloseRequested`：

1. `event.preventDefault()` 阻止销毁。
2. 若内部标志 `allowClose=true`（真退出路径）则不拦截，允许关闭。
3. 否则打开应用内确认对话框（非系统 `confirm`，以支持三按钮样式）。

Rust 侧同步在 `on_window_event(CloseRequested)` 作为兜底：当 `ALLOW_EXIT` 为 false 时 `api.prevent_close()` + 向前端 emit `close-requested`（若前端未挂上）。首选前端对话框，避免双弹。

### 托盘（Rust）

- `Cargo.toml`: `tauri = { version = "2", features = ["tray-icon"] }`
- `setup` 中 `TrayIconBuilder`：默认窗口图标、tooltip、菜单「显示窗口」「退出」
- 左键 Up → `show_main_window`
- 菜单 show → `show_main_window`；quit → 设 `ALLOW_EXIT` 后 `app.exit(0)`

命令：

| 命令 | 作用 |
|------|------|
| `hide_to_tray` | hide 主窗；确保托盘已创建 |
| `show_main_window` | show + unminimize + set_focus |
| `quit_app` | 设允许退出标志后关闭主窗 / `app.exit(0)` |

### UI 重建协同

`recreate_ui_webview`：

1. 记录 `was_hidden = !window.is_visible()`（在 destroy 前）。
2. 现有 keepalive / prevent_exit 不变。
3. 重建后：若 `was_hidden`，则 `hide()` 且不要 `set_focus`；否则保持现有 show/focus/minimize 逻辑。
4. CloseRequested 兜底在 `UI_RECREATING==true` 时不 `prevent_close`（允许 destroy）。

### 二次启动（Windows）

`acquire_single_instance` 发现已存在实例时：向已有进程发唤起信号（例如命名事件 / 窗口消息）。最小实现：用已存在的 mutex 名配套一个 named event；主实例后台线程 wait 后 `show_main_window`。若唤起失败，至少不再静默 return（可 eprintln）。macOS 可后续用 `tauri-plugin-single-instance`；本次以 Windows 为主。

### 对话框 UI

应用内 modal（Svelte）：

- 标题：关闭 Coding Tools MCP?
- 说明：选择后台运行可隐藏窗口并保持 MCP、Actions 和隧道服务继续运行，之后可通过系统托盘重新打开。
- 按钮：取消 | 后台运行 | 直接关闭（危险样式）

挂在根 layout，全局一次。

## 关键决策

1. **应用内三按钮 modal，不用系统 dialog**：系统 ask/confirm 难以稳定做出与设计稿一致的三按钮。
2. **真退出设允许标志再关窗**：避免 CloseRequested 再次拦截。
3. **托盘隐藏用 hide 而非 minimize**：与 taskbar 最小化语义分离，减少与 0.1.32/33 修复路径冲突。
4. **二次启动 Should**：尽力做 Windows 唤起，不阻塞主路径。

## 文件结构

| 文件 | 变更 |
|------|------|
| `src-tauri/Cargo.toml` | tray-icon feature |
| `src-tauri/src/lib.rs` | tray setup、window event、commands |
| `src-tauri/src/commands/window_chrome.rs`（新建） | hide/show/quit + 标志 |
| `src-tauri/src/commands/ui_memory.rs` | was_hidden 恢复 |
| `src-tauri/capabilities/default.json` | 如需补 core 权限 |
| `src/lib/components/CloseConfirmDialog.svelte`（新建） | 三按钮 |
| `src/lib/close-guard.ts`（新建） | onCloseRequested 绑定 |
| `src/lib/api/window-chrome.ts`（新建） | invoke 封装 |
| `src/routes/+layout.svelte` | 挂载 dialog + close-guard |
| `src/app.css` | modal 样式 |
