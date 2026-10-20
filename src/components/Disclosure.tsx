/** The open/closed triangle of a sidebar tree row. */
import { DISCLOSURE } from "./treeStyles";

export function Disclosure({ open }: { open: boolean }) {
  return (
    <span className={DISCLOSURE} aria-hidden="true">
      <svg viewBox="0 0 10 10" width="9" height="9" aria-hidden="true">
        <path d={open ? "M1 2.5h8L5 8z" : "M2.5 1v8L8 5z"} fill="currentColor" />
      </svg>
    </span>
  );
}
