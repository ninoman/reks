/** One setting in a Settings tab's card: a title, an optional hint, then its control. */
import type { ReactNode } from "react";

/** The item's padding, for settings that lay themselves out (`OutputSettings`). */
export const settingItemClass = "px-4 py-3.5";

export function SettingItem({
  title,
  hint,
  children,
}: {
  title: string;
  hint?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div className={settingItemClass}>
      <h5 className="m-0 text-sm font-semibold">{title}</h5>
      {hint !== undefined && <p className="mt-0.5 mb-0 text-xs text-fg-muted">{hint}</p>}
      <div className="mt-2.5">{children}</div>
    </div>
  );
}
