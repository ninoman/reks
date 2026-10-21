/** Shows the current toast and dismisses it after a while (paused while hovered or focused). */
import { useEffect, useState } from "react";

import { TOAST_MS, useToasts } from "../store/toasts";
import { Toast } from "./ui/Toast";

export function ToastHost() {
  const toast = useToasts((s) => s.toast);
  const dismiss = useToasts((s) => s.dismiss);
  const [held, setHeld] = useState(false);

  const id = toast?.id;
  useEffect(() => {
    if (id === undefined || held) return;
    const timer = setTimeout(dismiss, TOAST_MS);
    return () => clearTimeout(timer);
  }, [id, held, dismiss]);

  if (toast === null) return null;
  const { action } = toast;
  return (
    <Toast
      key={toast.id}
      onDismiss={dismiss}
      action={
        action === undefined
          ? undefined
          : {
              label: action.label,
              onClick: () => {
                action.run();
                dismiss();
              },
            }
      }
      onPointerEnter={() => setHeld(true)}
      onPointerLeave={() => setHeld(false)}
      onFocus={() => setHeld(true)}
      onBlur={() => setHeld(false)}
    >
      {toast.message}
    </Toast>
  );
}
