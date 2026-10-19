/** Search field; calls `onSearch` 50 ms after the last keystroke. */
import { useEffect, useRef, useState } from "react";

import { Input } from "./ui/Input";

export const SEARCH_DEBOUNCE_MS = 50;

export function SearchBox({ onSearch }: { onSearch: (text: string) => void }) {
  const [value, setValue] = useState("");
  const first = useRef(true);
  useEffect(() => {
    if (first.current) {
      first.current = false;
      return;
    }
    const t = setTimeout(() => onSearch(value), SEARCH_DEBOUNCE_MS);
    return () => clearTimeout(t);
  }, [value, onSearch]);
  return (
    <Input
      variant="search"
      size="sm"
      className="w-65"
      placeholder="Search"
      aria-label="Search"
      value={value}
      onChange={(e) => setValue(e.target.value)}
    />
  );
}
