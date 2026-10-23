/**
 * A caption that picks one of a few values: it reads as a label (mixer
 * legend) with a chevron, and opens a menu of the choices. Each choice may
 * carry a hint, shown muted on the right.
 */
import { cn } from "../../lib/cn";
import { ChevronDownIcon } from "../icons";
import { captionVariants } from "./Caption";
import { focusRing } from "./control";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from "./DropdownMenu";

export interface CaptionSelectOption<T extends string> {
  value: T;
  label: string;
  hint?: string;
}

export interface CaptionSelectProps<T extends string> {
  value: T;
  options: readonly CaptionSelectOption<T>[];
  onValueChange: (value: T) => void;
  "aria-label": string;
  className?: string;
}

export function CaptionSelect<T extends string>({
  value,
  options,
  onValueChange,
  "aria-label": ariaLabel,
  className,
}: CaptionSelectProps<T>) {
  const current = options.find((o) => o.value === value);
  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        aria-label={`${ariaLabel}: ${current?.label ?? ""}`}
        className={cn(
          captionVariants({ tone: "default" }),
          "inline-flex h-(--ctl-h-xs) cursor-pointer items-center rounded-xs hover:text-accent data-[state=open]:text-accent",
          "[&_svg]:size-(--ctl-icon-xs) [&_svg]:text-fg-muted",
          focusRing,
          className,
        )}
      >
        {current?.label}
        <ChevronDownIcon />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuRadioGroup value={value} onValueChange={(v) => onValueChange(v as T)}>
          {options.map((o) => (
            <DropdownMenuRadioItem key={o.value} value={o.value}>
              {o.label}
              {o.hint !== undefined && (
                <span className="ml-auto pl-3 text-fg-subtle text-xs">{o.hint}</span>
              )}
            </DropdownMenuRadioItem>
          ))}
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
