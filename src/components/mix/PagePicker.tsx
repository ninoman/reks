/** ◀ page ▶: steps through pages of pad sizes. */
import { ChevronRightIcon } from "../icons";
import { IconButton } from "../ui/IconButton";

export interface PagePickerProps {
  /** 0-based. */
  page: number;
  pages: number;
  disabled: boolean;
  /** Names the buttons: "Deck 2 jump sizes". */
  label: string;
  onPage: (page: number) => void;
}

export function PagePicker({ page, pages, disabled, label, onPage }: PagePickerProps) {
  return (
    <div className="flex shrink-0 items-center gap-0.5">
      <IconButton
        size="xs"
        variant="ghost"
        aria-label={`${label}: previous page`}
        disabled={disabled || page <= 0}
        onClick={() => onPage(page - 1)}
        className="[&_svg]:rotate-180"
      >
        <ChevronRightIcon />
      </IconButton>
      {/* The arrows are enough on narrow pads (`DeckPads` is the container): the tabs get the room. */}
      <span
        className="hidden font-mono text-2xs text-fg-subtle tabular-nums @[12rem]:inline"
        aria-hidden="true"
      >
        {disabled ? "–" : `${page + 1}/${pages}`}
      </span>
      <IconButton
        size="xs"
        variant="ghost"
        aria-label={`${label}: next page`}
        disabled={disabled || page >= pages - 1}
        onClick={() => onPage(page + 1)}
      >
        <ChevronRightIcon />
      </IconButton>
    </div>
  );
}
