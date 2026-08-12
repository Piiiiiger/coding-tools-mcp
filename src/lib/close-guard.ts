import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";

export type CloseDialogOpener = () => void;

/**
 * Intercept the main window close button and open the confirm dialog instead.
 * Returns an unsubscribe function.
 */
export function startCloseGuard(openDialog: CloseDialogOpener): () => void {
  if (typeof window === "undefined") {
    return () => {};
  }

  let unlistenWindow: (() => void) | undefined;
  let unlistenEvent: (() => void) | undefined;
  let disposed = false;

  void (async () => {
    try {
      unlistenWindow = await getCurrentWindow().onCloseRequested(async (event) => {
        event.preventDefault();
        openDialog();
      });
    } catch {
      // Non-Tauri / web preview: ignore.
    }

    try {
      unlistenEvent = await listen("close-requested", () => {
        openDialog();
      });
    } catch {
      // ignore
    }

    if (disposed) {
      unlistenWindow?.();
      unlistenEvent?.();
    }
  })();

  return () => {
    disposed = true;
    unlistenWindow?.();
    unlistenEvent?.();
  };
}
