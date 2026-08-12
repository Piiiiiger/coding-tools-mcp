<script lang="ts">
  import { hideToTray, quitApp } from "$lib/api/window-chrome";

  interface Props {
    open?: boolean;
    onCancel?: () => void;
  }

  let { open = false, onCancel }: Props = $props();

  let busy = $state(false);

  async function runBackground() {
    if (busy) return;
    busy = true;
    try {
      await hideToTray();
      onCancel?.();
    } catch (error) {
      console.error("[close-confirm] hide_to_tray failed", error);
    } finally {
      busy = false;
    }
  }

  async function runQuit() {
    if (busy) return;
    busy = true;
    try {
      await quitApp();
    } catch (error) {
      console.error("[close-confirm] quit_app failed", error);
      busy = false;
    }
  }
</script>

{#if open}
  <div class="tx-close-overlay" role="presentation">
    <div
      class="tx-close-dialog"
      role="alertdialog"
      aria-modal="true"
      aria-labelledby="tx-close-title"
      aria-describedby="tx-close-desc"
    >
      <h2 id="tx-close-title" class="tx-close-title">关闭 Coding Tools MCP?</h2>
      <p id="tx-close-desc" class="tx-close-desc">
        选择后台运行可隐藏窗口并保持 MCP、Actions 和隧道服务继续运行，之后可通过系统托盘重新打开。
      </p>
      <div class="tx-close-actions">
        <button
          type="button"
          class="tx-btn-ghost"
          disabled={busy}
          onclick={() => onCancel?.()}
        >
          取消
        </button>
        <button
          type="button"
          class="tx-btn-ghost"
          disabled={busy}
          onclick={() => void runBackground()}
        >
          后台运行
        </button>
        <button
          type="button"
          class="tx-btn-primary tx-btn-danger"
          disabled={busy}
          onclick={() => void runQuit()}
        >
          直接关闭
        </button>
      </div>
    </div>
  </div>
{/if}
