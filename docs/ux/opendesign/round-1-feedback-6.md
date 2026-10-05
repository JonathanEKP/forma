Calm retested: scrollbar, bottom safe area, approval bubble, panel icon, per-layer properties and the issues popover all work. Two small things left:

1. The zoom control says "100%" while the artboard is fitted at roughly a quarter of its size. Show the real fitted percentage (e.g. "27%"), and make the label a button that offers "Fit" / "100%".
2. Right after switching scenario, the approval card and its text render greyed out for about half a second to a second before becoming solid (a colour/opacity transition on its children, not the 120 ms card animation). Remove that transition; the card should appear solid immediately (≤120 ms, and none with prefers-reduced-motion).
