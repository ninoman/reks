/** A small right-click menu at a screen point. Closes on Escape, outside click or a choice. */
import { useEffect, useRef } from "react";

export interface MenuItem {
  label: string;
  onSelect: () => void;
  disabled?: boolean;
}

export interface MenuState {
  x: number;
  y: number;
  items: MenuItem[];
}

export function ContextMenu({ menu, onClose }: { menu: MenuState; onClose: () => void }) {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    ref.current?.querySelector("button")?.focus();
    const onDown = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) onClose();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [onClose]);

  return (
    <div ref={ref} className="context-menu" role="menu" style={{ left: menu.x, top: menu.y }}>
      {menu.items.map((item) => (
        <button
          key={item.label}
          type="button"
          role="menuitem"
          disabled={item.disabled}
          onClick={() => {
            onClose();
            item.onSelect();
          }}
        >
          {item.label}
        </button>
      ))}
    </div>
  );
}
