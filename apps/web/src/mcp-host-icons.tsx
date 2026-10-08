import type { McpHostIconName } from "./mcp-hosts";

interface McpHostIconPaths {
  readonly viewBox: string;
  readonly paths: readonly string[];
  readonly fillRule?: "evenodd";
}

const ICON_PATHS: Record<McpHostIconName, McpHostIconPaths> = {
  anthropic: {
    viewBox: "0 0 24 24",
    paths: ["M17.3041 3.541h-3.6718l6.696 16.918H24Zm-10.6082 0L0 20.459h3.7442l1.3693-3.5527h7.0052l1.3693 3.5528h3.7442L10.5363 3.5409Zm-.3712 10.2232 2.2914-5.9456 2.2914 5.9456Z"],
  },
  "openai-neutral": {
    viewBox: "0 0 24 24",
    paths: ["M12 2.5a1 1 0 0 1 .95.68l1.13 3.48 3.49 1.13a1 1 0 0 1 0 1.9l-3.49 1.13-1.13 3.49a1 1 0 0 1-1.9 0l-1.13-3.49-3.48-1.13a1 1 0 0 1 0-1.9l3.48-1.13 1.13-3.48a1 1 0 0 1 .95-.68Zm6.5 11.5a.75.75 0 0 1 .71.51l.55 1.69 1.69.55a.75.75 0 0 1 0 1.42l-1.69.55-.55 1.69a.75.75 0 0 1-1.42 0l-.55-1.69-1.69-.55a.75.75 0 0 1 0-1.42l1.69-.55.55-1.69a.75.75 0 0 1 .71-.51Z"],
  },
  opencode: {
    viewBox: "0 0 24 24",
    paths: ["M22 24H2V0h20zM17 4.8H7v14.4h10z"],
    fillRule: "evenodd",
  },
  cursor: {
    viewBox: "0 0 24 24",
    paths: ["M11.503.131 1.891 5.678a.84.84 0 0 0-.42.726v11.188c0 .3.162.575.42.724l9.609 5.55a1 1 0 0 0 .998 0l9.61-5.55a.84.84 0 0 0 .42-.724V6.404a.84.84 0 0 0-.42-.726L12.497.131a1.01 1.01 0 0 0-.996 0M2.657 6.338h18.55c.263 0 .43.287.297.515L12.23 22.918c-.062.107-.229.064-.229-.06V12.335a.59.59 0 0 0-.295-.51l-9.11-5.257c-.109-.063-.064-.23.061-.23"],
  },
  antigravity: {
    viewBox: "0 0 180 180",
    paths: ["M144.248 149.062C151.748 154.688 162.998 150.938 152.685 140.625C121.748 110.625 128.31 28.125 89.8727 28.125C51.4352 28.125 57.9977 110.625 27.0602 140.625C15.8102 151.875 27.9977 154.688 35.4977 149.062C64.5602 129.375 62.6852 94.6875 89.8727 94.6875C117.06 94.6875 115.185 129.375 144.248 149.062Z"],
  },
  "neutral-app": {
    viewBox: "0 0 24 24",
    paths: ["M5 3h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2Zm0 5v11h14V8H5Zm2-3a1 1 0 1 0 0 2 1 1 0 0 0 0-2Zm4 0a1 1 0 1 0 0 2 1 1 0 0 0 0-2Z"],
    fillRule: "evenodd",
  },
  plug: {
    viewBox: "0 0 24 24",
    paths: ["M8 2h2v5H8zm6 0h2v5h-2zM5 7h14v2a7 7 0 0 1-6 6.93V22h-2v-6.07A7 7 0 0 1 5 9V7Z"],
  },
};

export function McpHostIcon({ name }: { name: McpHostIconName }) {
  const icon = ICON_PATHS[name];

  return (
    <svg
      aria-hidden="true"
      className="mcp-host-icon h-4 w-4 shrink-0"
      focusable="false"
      fill="currentColor"
      viewBox={icon.viewBox}
    >
      {icon.paths.map((path) => <path key={path} d={path} fillRule={icon.fillRule} />)}
    </svg>
  );
}
