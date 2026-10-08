export interface McpCodeSample {
  readonly label: string;
  readonly accessibleName: string;
  readonly copyName: string;
  readonly content: (mcpUrl: string) => string;
}

export interface McpTokenPath {
  readonly steps: readonly string[];
  readonly samples: readonly McpCodeSample[];
  readonly warning?: string;
}

export type McpHostGroupId = "browser" | "terminal" | "editor";
export type McpHostIconName = "anthropic" | "openai-neutral" | "opencode" | "cursor" | "antigravity" | "neutral-app" | "plug";

export interface McpHost {
  readonly id: string;
  readonly name: string;
  readonly icon: McpHostIconName;
  readonly group?: McpHostGroupId;
  readonly requirement?: string;
  readonly steps: readonly string[];
  readonly samples: readonly McpCodeSample[];
  readonly officialGuide?: string;
  readonly token?: McpTokenPath;
}

export const MCP_HOST_GROUPS: readonly { readonly id: McpHostGroupId; readonly heading: string }[] = [
  { id: "browser", heading: "In your browser" },
  { id: "terminal", heading: "In your terminal" },
  { id: "editor", heading: "In your editor" },
];

export const MCP_HOSTS: readonly McpHost[] = [
  {
    id: "claude",
    name: "Claude",
    icon: "anthropic",
    group: "browser",
    requirement: "Free allows one connector; Team and Enterprise owners can add connectors for their organization.",
    steps: [
      "Open Customize → Connectors → + Add → Add custom connector.",
      "Name it Forma, paste this URL, and choose Continue.",
      "Choose Sign in now → Register automatically, then sign in in your browser.",
      "For Team or Enterprise, members open Customize → Connectors → Connect.",
    ],
    samples: [{
      label: "Forma MCP URL",
      accessibleName: "Claude MCP URL",
      copyName: "Copy Claude MCP URL",
      content: (mcpUrl) => mcpUrl,
    }],
    officialGuide: "https://support.claude.com/en/articles/11175166-get-started-with-custom-connectors-using-remote-mcp",
  },
  {
    id: "chatgpt",
    name: "ChatGPT",
    icon: "openai-neutral",
    group: "browser",
    requirement: "Editing needs a Business, Enterprise or Edu plan; on Pro, Forma is read-only. A workspace admin publishes the app.",
    steps: [
      "Enable developer mode: Pro uses Settings → Apps; Business owners/admins use Workspace settings → Permissions & Roles → Connected Data; Enterprise/Edu needs RBAC access, then Settings → Apps → Advanced Settings.",
      "Choose Settings → Apps → Create (admins: Workspace settings → Apps → Create), enter Forma and its URL, then choose the browser sign-in option.",
      "Choose Scan Tools, sign in in your browser, then choose Create.",
      "A Business owner/admin or authorized Enterprise/Edu publisher publishes from Workspace settings → Apps → Drafts.",
    ],
    samples: [{
      label: "Forma MCP URL",
      accessibleName: "ChatGPT MCP URL",
      copyName: "Copy ChatGPT MCP URL",
      content: (mcpUrl) => mcpUrl,
    }],
    officialGuide: "https://help.openai.com/en/articles/12584461-developer-mode-and-mcp-apps-in-chatgpt",
  },
  {
    id: "claude-code",
    name: "Claude Code",
    icon: "anthropic",
    group: "terminal",
    steps: [
      "In your project directory, register Forma with this command.",
      "Run the browser sign-in command, then sign in in your browser.",
      "Approve the project config when prompted, then check the connection.",
    ],
    samples: [{
      label: "Commands",
      accessibleName: "Claude Code sign-in and connection check commands",
      copyName: "Copy Claude Code sign-in and connection check commands",
      content: (mcpUrl) => `claude mcp add --transport http --scope project forma "${mcpUrl}"\nclaude mcp login forma\nclaude mcp get forma`,
    }],
    officialGuide: "https://code.claude.com/docs/en/mcp",
    token: {
      steps: [
        "Create a personal access token below, then enter it at a shell prompt.",
        "Add the Authorization header shown below to the generated .mcp.json and approve the project config.",
      ],
      samples: [
        {
          label: "Read the token into this shell",
          accessibleName: "Claude Code token environment command",
          copyName: "Copy Claude Code token environment command",
          content: () => "read -rsp 'Forma PAT: ' FORMA_TOKEN; echo; export FORMA_TOKEN",
        },
        {
          label: "Project config header",
          accessibleName: "Claude Code token config",
          copyName: "Copy Claude Code token config",
          content: (mcpUrl) => JSON.stringify({
            mcpServers: {
              forma: {
                type: "http",
                url: mcpUrl,
                headers: { Authorization: "Bearer ${FORMA_TOKEN}" },
              },
            },
          }, null, 2),
        },
      ],
    },
  },
  {
    id: "codex-cli",
    name: "Codex CLI",
    icon: "openai-neutral",
    group: "terminal",
    steps: [
      "Register Forma with this command.",
      "Run the browser sign-in command, then sign in in your browser.",
      "Check the connection with the final command in the block, or run /mcp in the TUI.",
    ],
    samples: [{
      label: "Commands",
      accessibleName: "Codex CLI sign-in and connection check commands",
      copyName: "Copy Codex CLI sign-in and connection check commands",
      content: (mcpUrl) => `codex mcp add forma --url "${mcpUrl}"\ncodex mcp login forma\ncodex mcp list`,
    }],
    officialGuide: "https://developers.openai.com/codex/mcp",
    token: {
      steps: [
        "Create a personal access token below, then enter it at a shell prompt.",
        "Register Forma with the command below. Codex stores the variable name, not its value.",
      ],
      samples: [
        {
          label: "Read the token into this shell",
          accessibleName: "Codex CLI token environment command",
          copyName: "Copy Codex CLI token environment command",
          content: () => "read -rsp 'Forma PAT: ' FORMA_TOKEN; echo; export FORMA_TOKEN",
        },
        {
          label: "Token setup command",
          accessibleName: "Codex CLI token setup command",
          copyName: "Copy Codex CLI token setup command",
          content: (mcpUrl) => `codex mcp add forma --url "${mcpUrl}" --bearer-token-env-var FORMA_TOKEN`,
        },
      ],
    },
  },
  {
    id: "opencode",
    name: "OpenCode",
    icon: "opencode",
    group: "terminal",
    steps: [
      "Add this configuration to opencode.json in your project root.",
      "Run the sign-in command, then sign in in your browser.",
      "Check the connection with the listed commands.",
    ],
    samples: [{
      label: "Project config",
      accessibleName: "OpenCode project config",
      copyName: "Copy OpenCode project config",
      content: (mcpUrl) => JSON.stringify({
        $schema: "https://opencode.ai/config.json",
        mcp: { forma: { type: "remote", url: mcpUrl, enabled: true } },
      }, null, 2),
    }, {
      label: "Sign in and check",
      accessibleName: "OpenCode sign-in and connection check commands",
      copyName: "Copy OpenCode sign-in and connection check commands",
      content: () => "opencode mcp auth forma\nopencode mcp list\nopencode mcp debug forma",
    }],
    officialGuide: "https://opencode.ai/docs/mcp-servers/",
    token: {
      steps: [
        "Create a personal access token below, then enter it at a shell prompt.",
        "Use this project config; it references the environment variable rather than its value.",
      ],
      samples: [
        {
          label: "Read the token into this shell",
          accessibleName: "OpenCode token environment command",
          copyName: "Copy OpenCode token environment command",
          content: () => "read -rsp 'Forma PAT: ' FORMA_TOKEN; echo; export FORMA_TOKEN",
        },
        {
          label: "Token project config",
          accessibleName: "OpenCode token config",
          copyName: "Copy OpenCode token config",
          content: (mcpUrl) => JSON.stringify({
            $schema: "https://opencode.ai/config.json",
            mcp: {
              forma: {
                type: "remote",
                url: mcpUrl,
                enabled: true,
                oauth: false,
                headers: { Authorization: "Bearer {env:FORMA_TOKEN}" },
              },
            },
          }, null, 2),
        },
      ],
    },
  },
  {
    id: "cursor",
    name: "Cursor",
    icon: "cursor",
    group: "editor",
    steps: [
      "Add this configuration to .cursor/mcp.json in your project root.",
      "Open Customize and check that Forma is listed.",
      "Choose Forma, then sign in in your browser.",
    ],
    samples: [{
      label: "Project config",
      accessibleName: "Cursor project config",
      copyName: "Copy Cursor project config",
      content: (mcpUrl) => JSON.stringify({ mcpServers: { forma: { url: mcpUrl } } }, null, 2),
    }],
    officialGuide: "https://cursor.com/docs/mcp",
    token: {
      steps: [
        "Create a personal access token below, then enter it at a shell prompt.",
        "Add the header to .cursor/mcp.json; the file keeps an environment reference, not the token value.",
      ],
      samples: [
        {
          label: "Read the token into this shell",
          accessibleName: "Cursor token environment command",
          copyName: "Copy Cursor token environment command",
          content: () => "read -rsp 'Forma PAT: ' FORMA_TOKEN; echo; export FORMA_TOKEN",
        },
        {
          label: "Token project config",
          accessibleName: "Cursor token config",
          copyName: "Copy Cursor token config",
          content: (mcpUrl) => JSON.stringify({
            mcpServers: {
              forma: {
                url: mcpUrl,
                headers: { Authorization: "Bearer ${env:FORMA_TOKEN}" },
              },
            },
          }, null, 2),
        },
      ],
    },
  },
  {
    id: "vscode-copilot",
    name: "VS Code (GitHub Copilot)",
    icon: "neutral-app",
    group: "editor",
    requirement: "VS Code 1.99+; Business/Enterprise needs Copilot access and the organization MCP policy enabled.",
    steps: [
      "Open the Command Palette and run MCP: Add Server.",
      "Choose HTTP, enter this URL, name the server forma, and save to your workspace .mcp.json.",
      "Start Forma, sign in in your browser, then run MCP: List Servers.",
    ],
    samples: [{
      label: "Forma MCP URL",
      accessibleName: "VS Code MCP URL",
      copyName: "Copy VS Code MCP URL",
      content: (mcpUrl) => mcpUrl,
    }],
    officialGuide: "https://code.visualstudio.com/docs/agent-customization/mcp-servers",
    token: {
      steps: [
        "Create a personal access token below. VS Code asks for it securely when the server connects.",
        "In the workspace .mcp.json, use a password input for the Authorization header.",
      ],
      samples: [
        {
          label: "Token project config",
          accessibleName: "VS Code token config",
          copyName: "Copy VS Code token config",
          content: (mcpUrl) => JSON.stringify({
            mcpServers: {
              forma: {
                type: "http",
                url: mcpUrl,
                headers: { Authorization: "Bearer ${input:forma-token}" },
              },
            },
            inputs: [{ id: "forma-token", type: "promptString", description: "Forma personal access token", password: true }],
          }, null, 2),
        },
      ],
    },
  },
  {
    id: "antigravity",
    name: "Antigravity",
    icon: "antigravity",
    group: "editor",
    steps: [
      "Add this to ~/.gemini/config/mcp_config.json or your project's .agents/mcp_config.json. It works in Antigravity desktop, IDE, and CLI.",
      "In Agent settings → Customizations, choose Authenticate next to Forma. Sign in in your browser, then paste the code back and submit.",
      "In the CLI, run /mcp to check the connection.",
    ],
    samples: [{
      label: "MCP config",
      accessibleName: "Antigravity MCP config",
      copyName: "Copy Antigravity MCP config",
      content: (mcpUrl) => JSON.stringify({ mcpServers: { forma: { serverUrl: mcpUrl } } }, null, 2),
    }],
    officialGuide: "https://antigravity.google/docs/mcp",
    token: {
      steps: [
        "Create a personal access token below.",
        "Add it as the Authorization header value in the config below. Replace the placeholder before using it.",
      ],
      samples: [{
        label: "Token config",
        accessibleName: "Antigravity token config",
        copyName: "Copy Antigravity token config",
        content: (mcpUrl) => JSON.stringify({
          mcpServers: {
            forma: {
              serverUrl: mcpUrl,
              headers: { Authorization: "Bearer YOUR_FORMA_TOKEN" },
            },
          },
        }, null, 2),
      }],
      warning: "Google's docs show bearer tokens as config values; no environment reference is documented. Prefer browser sign-in and avoid saving a real token in shared config.",
    },
  },
];

export const MCP_OTHER_HOST: McpHost = {
  id: "other-app",
  name: "Other app",
  icon: "plug",
  steps: [
    "Add the entry below under mcpServers as a remote Streamable HTTP server.",
    "If your app offers sign-in in a browser, choose it; Forma advertises it automatically.",
    'Otherwise send a personal access token as Authorization: Bearer <token>. Keep it as an environment reference or placeholder. Some apps use "serverUrl" instead of "url", or also need "type": "http".',
  ],
  samples: [{
    label: "Most common config",
    accessibleName: "Other app MCP config",
    copyName: "Copy Other app MCP config",
    content: (mcpUrl) => JSON.stringify({
      mcpServers: {
        forma: {
          url: mcpUrl,
          headers: { Authorization: "Bearer ${FORMA_TOKEN}" },
        },
      },
    }, null, 2),
  }],
};

export const MCP_HOSTS_IN_TAB_ORDER: readonly McpHost[] = [
  ...MCP_HOST_GROUPS.flatMap((group) => MCP_HOSTS.filter((host) => host.group === group.id)),
  MCP_OTHER_HOST,
];
