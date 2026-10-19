/** Small line icons (16 px, `currentColor`) for toolbar buttons. */

export function InfoIcon() {
  return (
    <svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true">
      <circle cx="8" cy="8" r="6.5" fill="none" stroke="currentColor" strokeWidth="1.3" />
      <rect x="7.3" y="7" width="1.4" height="4.5" rx="0.7" fill="currentColor" />
      <circle cx="8" cy="4.9" r="0.9" fill="currentColor" />
    </svg>
  );
}

export function FilterIcon() {
  return (
    <svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true">
      <path
        d="M2 3h12l-4.6 5.4V13l-2.8-1.4V8.4z"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.3"
        strokeLinejoin="round"
      />
    </svg>
  );
}
