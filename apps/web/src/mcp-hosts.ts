export interface McpCodeSample {
  readonly label: string;
  readonly accessibleName: string;
  readonly copyName: string;
  readonly content: (mcpUrl: string) => string;
}

export interface McpSetupStep {
  readonly text: string;
  readonly code?: McpCodeSample;
  readonly inlineCode?: readonly string[];
  readonly linksToTokenForm?: boolean;
}

export interface McpTokenPath {
  readonly steps: readonly McpSetupStep[];
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
  readonly steps: readonly McpSetupStep[];
  readonly officialGuide?: string;
  readonly token?: McpTokenPath;
}

export const MCP_HOST_GROUPS: readonly { readonly id: McpHostGroupId; readonly heading: string }[] = [
  { id: "browser", heading: "In your browser" },
  { id: "terminal", heading: "In your terminal" },
  { id: "editor", heading: "In your editor" },
];

const tokenShellStep: McpSetupStep = {
  text: "Create a personal access token below. Read it into your shell without displaying it.",
  linksToTokenForm: true,
  inlineCode: ["FORMA_TOKEN"],
  code: {
    label: "Terminal · Bash",
    accessibleName: "Token environment command",
    copyName: "Copy token environment command",
    content: () => "read -rsp 'Forma PAT: ' FORMA_TOKEN; echo; export FORMA_TOKEN",
  },
};

export const MCP_HOSTS: readonly McpHost[] = [
  {
    id: "claude",
    name: "Claude",
    icon: "anthropic",
    group: "browser",
    requirement: "Free allows one connector; Team and Enterprise owners can add connectors for their organization.",
    steps: [
      { text: "Open Customize → Connectors → + Add → Add custom connector." },
      {
        text: "Name it Forma, paste this URL, and choose Continue.",
        code: {
          label: "Forma MCP URL",
          accessibleName: "Claude MCP URL",
          copyName: "Copy Claude MCP URL",
          content: (mcpUrl) => mcpUrl,
        },
      },
      { text: "Choose Sign in now → Register automatically, then sign in in your browser." },
      { text: "For Team or Enterprise, members open Customize → Connectors → Connect." },
    ],
    officialGuide: "https://support.claude.com/en/articles/11175166-get-started-with-custom-connectors-using-remote-mcp",
  },
  {
    id: "chatgpt",
    name: "ChatGPT",
    icon: "openai-neutral",
    group: "browser",
    requirement: "Editing needs a Business, Enterprise or Edu plan; on Pro, Forma is read-only. A workspace admin publishes the app.",
    steps: [
      {
        text: "Enable developer mode. Pro: Settings → Apps. Business owners/admins: Workspace settings → Permissions & Roles → Connected Data. Enterprise/Edu: get RBAC access, then Settings → Apps → Advanced Settings.",
      },
      {
        text: "Open Settings → Apps → Create (admins: Workspace settings → Apps → Create). Enter Forma and this URL, then choose browser sign-in.",
        code: {
          label: "Forma MCP URL",
          accessibleName: "ChatGPT MCP URL",
          copyName: "Copy ChatGPT MCP URL",
          content: (mcpUrl) => mcpUrl,
        },
      },
      { text: "Choose Scan Tools, sign in in your browser, then choose Create." },
      { text: "A Business owner/admin or authorized Enterprise/Edu publisher publishes from Workspace settings → Apps → Drafts." },
    ],
    officialGuide: "https://help.openai.com/en/articles/12584461-developer-mode-and-mcp-apps-in-chatgpt",
  },
  {
    id: "claude-code",
    name: "Claude Code",
    icon: "anthropic",
    group: "terminal",
    steps: [
      {
        text: "In your project directory, register Forma with this command.",
        code: {
          label: "Commands",
          accessibleName: "Claude Code registration command",
          copyName: "Copy Claude Code registration command",
          content: (mcpUrl) => `claude mcp add --transport http --scope project forma "${mcpUrl}"`,
        },
      },
      {
        text: "Run the browser sign-in command, then sign in in your browser.",
        code: {
          label: "Commands",
          accessibleName: "Claude Code sign-in command",
          copyName: "Copy Claude Code sign-in command",
          content: () => "claude mcp login forma",
        },
      },
      {
        text: "Approve the project config when prompted, then check the connection.",
        code: {
          label: "Commands",
          accessibleName: "Claude Code connection check command",
          copyName: "Copy Claude Code connection check command",
          content: () => "claude mcp get forma",
        },
      },
    ],
    officialGuide: "https://code.claude.com/docs/en/mcp",
    token: {
      steps: [
        tokenShellStep,
        {
          text: "Add the Authorization header to your project config.",
          inlineCode: ["Authorization", ".mcp.json"],
          code: {
            label: ".mcp.json",
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
        },
        {
          text: "Start Claude Code from this shell, approve the project config, then check the connection.",
          code: {
            label: "Commands",
            accessibleName: "Claude Code token connection check command",
            copyName: "Copy Claude Code token connection check command",
            content: () => "claude mcp get forma",
          },
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
      {
        text: "Register Forma.",
        code: {
          label: "Commands",
          accessibleName: "Codex CLI registration command",
          copyName: "Copy Codex CLI registration command",
          content: (mcpUrl) => `codex mcp add forma --url "${mcpUrl}"`,
        },
      },
      {
        text: "Sign in in your browser.",
        code: {
          label: "Commands",
          accessibleName: "Codex CLI sign-in command",
          copyName: "Copy Codex CLI sign-in command",
          content: () => "codex mcp login forma",
        },
      },
      {
        text: "Check the connection.",
        code: {
          label: "Commands",
          accessibleName: "Codex CLI connection check command",
          copyName: "Copy Codex CLI connection check command",
          content: () => "codex mcp list",
        },
      },
    ],
    officialGuide: "https://developers.openai.com/codex/mcp",
    token: {
      steps: [
        tokenShellStep,
        {
          text: "Register Forma using the environment variable.",
          inlineCode: ["FORMA_TOKEN"],
          code: {
            label: "Commands",
            accessibleName: "Codex CLI token setup command",
            copyName: "Copy Codex CLI token setup command",
            content: (mcpUrl) => `codex mcp add forma --url "${mcpUrl}" --bearer-token-env-var FORMA_TOKEN`,
          },
        },
        {
          text: "Start Codex from this shell, then check the connection.",
          code: {
            label: "Commands",
            accessibleName: "Codex CLI token connection check command",
            copyName: "Copy Codex CLI token connection check command",
            content: () => "codex mcp list",
          },
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
      {
        text: "Add Forma to an opencode.json project config.",
        inlineCode: ["opencode.json"],
        code: {
          label: "opencode.json",
          accessibleName: "OpenCode project config",
          copyName: "Copy OpenCode project config",
          content: (mcpUrl) => JSON.stringify({
            $schema: "https://opencode.ai/config.json",
            mcp: { forma: { type: "remote", url: mcpUrl, enabled: true } },
          }, null, 2),
        },
      },
      {
        text: "Run opencode mcp auth forma, then sign in in your browser.",
        inlineCode: ["opencode mcp auth forma"],
        code: {
          label: "Commands",
          accessibleName: "OpenCode sign-in command",
          copyName: "Copy OpenCode sign-in command",
          content: () => "opencode mcp auth forma",
        },
      },
      {
        text: "Check with opencode mcp list.",
        inlineCode: ["opencode mcp list"],
        code: {
          label: "Commands",
          accessibleName: "OpenCode connection check command",
          copyName: "Copy OpenCode connection check command",
          content: () => "opencode mcp list",
        },
      },
    ],
    officialGuide: "https://opencode.ai/docs/mcp-servers/",
    token: {
      steps: [
        tokenShellStep,
        {
          text: "Add the token header to the same project config.",
          code: {
            label: "opencode.json",
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
        },
        {
          text: "Start OpenCode from this shell and check the connection.",
          code: {
            label: "Commands",
            accessibleName: "OpenCode token connection check command",
            copyName: "Copy OpenCode token connection check command",
            content: () => "opencode mcp list",
          },
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
      {
        text: "Add Forma to a .cursor/mcp.json project config.",
        inlineCode: [".cursor/mcp.json"],
        code: {
          label: ".cursor/mcp.json",
          accessibleName: "Cursor project config",
          copyName: "Copy Cursor project config",
          content: (mcpUrl) => JSON.stringify({ mcpServers: { forma: { url: mcpUrl } } }, null, 2),
        },
      },
      { text: "Open Customize and check Forma is listed." },
      { text: "Choose Forma and sign in in your browser." },
    ],
    officialGuide: "https://cursor.com/docs/mcp",
    token: {
      steps: [
        tokenShellStep,
        {
          text: "Use this header in your project config. Launch Cursor from the shell where FORMA_TOKEN is available.",
          inlineCode: ["FORMA_TOKEN"],
          code: {
            label: ".cursor/mcp.json",
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
      { text: "Open the Command Palette and run MCP: Add Server.", inlineCode: ["MCP: Add Server"] },
      {
        text: "Choose HTTP, enter this URL, name it forma, and save to your workspace .mcp.json.",
        inlineCode: ["HTTP", ".mcp.json"],
        code: {
          label: "Forma MCP URL",
          accessibleName: "VS Code MCP URL",
          copyName: "Copy VS Code MCP URL",
          content: (mcpUrl) => mcpUrl,
        },
      },
      { text: "Start the server and complete browser sign-in. Check with MCP: List Servers.", inlineCode: ["MCP: List Servers"] },
    ],
    officialGuide: "https://code.visualstudio.com/docs/agent-customization/mcp-servers",
    token: {
      steps: [
        {
          text: "Create a personal access token below. Add a password input to your workspace config.",
          linksToTokenForm: true,
          inlineCode: [".mcp.json"],
          code: {
            label: ".mcp.json",
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
        },
        { text: "Start the server and enter the token when VS Code asks. Then run MCP: List Servers.", inlineCode: ["MCP: List Servers"] },
      ],
    },
  },
  {
    id: "antigravity",
    name: "Antigravity",
    icon: "antigravity",
    group: "editor",
    steps: [
      {
        text: "Add this to ~/.gemini/config/mcp_config.json or your project's .agents/mcp_config.json. It works in Antigravity desktop, IDE, and CLI.",
        inlineCode: ["~/.gemini/config/mcp_config.json", ".agents/mcp_config.json"],
        code: {
          label: "mcp_config.json",
          accessibleName: "Antigravity MCP config",
          copyName: "Copy Antigravity MCP config",
          content: (mcpUrl) => JSON.stringify({ mcpServers: { forma: { serverUrl: mcpUrl } } }, null, 2),
        },
      },
      { text: "In Agent settings → Customizations, choose Authenticate next to Forma. Sign in in your browser, then paste the code back and submit." },
      { text: "In the CLI, run /mcp to check the connection.", inlineCode: ["/mcp"] },
    ],
    officialGuide: "https://antigravity.google/docs/mcp",
    token: {
      warning: "Google's docs show bearer tokens as config values; no environment reference is documented. Prefer browser sign-in and avoid saving a real token in shared config.",
      steps: [
        {
          text: "Create a personal access token below. Your app accepts a bearer token as a config value.",
          linksToTokenForm: true,
          code: {
            label: "mcp_config.json · token placeholder",
            accessibleName: "Antigravity token config",
            copyName: "Copy Antigravity token config",
            content: (mcpUrl) => JSON.stringify({
              mcpServers: {
                forma: {
                  serverUrl: mcpUrl,
                  headers: { Authorization: "Bearer <token>" },
                },
              },
            }, null, 2),
          },
        },
      ],
    },
  },
];

export const MCP_OTHER_HOST: McpHost = {
  id: "other-app",
  name: "Other app",
  icon: "plug",
  steps: [
    {
      text: "Add the entry below under mcpServers as a remote Streamable HTTP server.",
      inlineCode: ["mcpServers", "Streamable HTTP"],
      code: {
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
      },
    },
    { text: "If your app offers sign-in in a browser, choose it; Forma advertises it automatically." },
    {
      text: 'Otherwise send a personal access token as Authorization: Bearer <token>. Keep it as an environment reference or placeholder. Some apps use "serverUrl" instead of "url", or also need "type": "http".',
      inlineCode: ["Authorization: Bearer <token>", '"serverUrl"', '"url"', '"type": "http"'],
    },
  ],
  officialGuide: "https://modelcontextprotocol.io/docs/develop/connect-remote-servers",
  token: {
    steps: [
      {
        text: "Create a personal access token below. Use the configuration format supported by your app.",
        linksToTokenForm: true,
        code: {
          label: "Most common config · token placeholder",
          accessibleName: "Other app token config",
          copyName: "Copy Other app token config",
          content: (mcpUrl) => JSON.stringify({
            mcpServers: {
              forma: {
                url: mcpUrl,
                headers: { Authorization: "Bearer <token>" },
              },
            },
          }, null, 2),
        },
      },
    ],
  },
};

export const MCP_HOSTS_IN_TAB_ORDER: readonly McpHost[] = [
  ...MCP_HOST_GROUPS.flatMap((group) => MCP_HOSTS.filter((host) => host.group === group.id)),
  MCP_OTHER_HOST,
];
