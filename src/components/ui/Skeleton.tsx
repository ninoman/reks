/** A placeholder block shown while content loads; size it with `className`. */
import type { ComponentProps } from "react";

import { cn } from "../../lib/cn";

export function Skeleton({ className, ...props }: ComponentProps<"div">) {
  return (
    <div
      aria-hidden="true"
      className={cn("animate-pulse rounded-md bg-surface-strong", className)}
      {...props}
    />
  );
}
