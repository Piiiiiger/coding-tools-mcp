# 设计文档：webview-recreate-memory-release

## 概述

在保持 Rust `AppState`（MCP/Actions/FRP）不变的前提下，通过销毁并重建主 `WebviewWindow` 强制回收 WebView2 进程内存。

## 技术方案

对应需求：FR-1、FR-2、FR-3。

1. 新增异步 Tauri 命令 `recreate_ui_webview`，在 Rust 侧完成 destroy → sleep → rebuild。
2. 前端 `reloadUiOnly` 统一 `invoke` 该命令；静默 hidden 与阈值超限共用此路径。
3. 阈值超限时直接自动重建（冷却控制），不再依赖用户点击 toast。

## D-1: `recreate_ui_webview` 命令

对应需求：FR-1。

- 位置: `src-tauri/src/commands/ui_memory.rs`
- 签名: `async fn recreate_ui_webview(app: AppHandle) -> AppResult<()>`
- 步骤:
  1. 解析主窗口：`get_webview_window("main")` 或唯一 webview 窗口。
  2. 记录 outer position/size、maximized/minimized。
  3. `window.destroy()`（比 `close` 更能确保 WebView2 退出）。
  4. `tokio::time::sleep(500ms)` 等待子进程退出。
  5. `WebviewWindowBuilder::from_config` + `build`；失败则 `WebviewUrl::App` 回退。
  6. 尽量恢复几何与 focus。
- 不调用任何 runtime/tunnel stop API。

## D-2: 前端守卫

对应需求：FR-2、FR-3。

- `reloadUiOnly(reason)` → `invoke("recreate_ui_webview")`，catch 后 `location.reload()`。
- `maybeWarnHighMemory`: 超过阈值且冷却通过则直接 `reloadUiOnly("auto-threshold")`。
- 静默 hidden 路径不变，仍调用 `reloadUiOnly`。

## 文件结构

```
src-tauri/src/commands/ui_memory.rs   # get_webview_memory_sample + recreate_ui_webview
src-tauri/src/commands/mod.rs         # export
src-tauri/src/lib.rs                  # invoke_handler
src/lib/api/ui-memory.ts              # recreateUiWebview()
src/lib/ui-memory-guard.ts            # reloadUiOnly / auto-threshold
src/routes/settings/general/+page.svelte  # 手动释放文案可微调
```

## 风险

- destroy 后 build 失败会导致无窗口：提供 from_config 失败时的硬编码回退构建。
- invoke 在窗口销毁时可能被前端判定为失败：属预期，以 Rust 侧成功为准。
- **0.1.30 回归**：销毁唯一窗口会触发 Tauri 进程退出，MCP/FRP 一并消失。0.1.31 起必须同时：`UI_RECREATING` + `ExitRequested.prevent_exit`，以及先创建隐藏 keepalive 窗口再 destroy 主窗口。
- **0.1.31 回归**：最小化触发静默重建后若再 `minimize()` 新窗口，Windows 上任务栏图标可能无法还原/最大化。0.1.32 起：destroy 前先 `unminimize`；重建后**禁止**再 minimize，只 `show`/`focus`，最大化状态可保留。
- **0.1.32 仍复现**：最小化时先读 `outer_position` 常为 Windows 哨兵坐标（约 -32000），重建后 `set_position` 把窗口放到屏幕外，任务栏点击像打不开。0.1.33：先 unminimize 再读几何；校验坐标；无效则 `center`；几何正常后再可选 minimize 回任务栏。
