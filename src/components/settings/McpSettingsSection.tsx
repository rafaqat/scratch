import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";
import { getSettings, updateSettings as saveSettings } from "../../services/notes";
import { Button } from "../ui";
import { Input } from "../ui";
import { CopyIcon, SpinnerIcon } from "../icons";
import type { McpStatus, Settings, WebhookLogEntry } from "../../types/note";

export function McpSettingsSection() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [mcpStatus, setMcpStatus] = useState<McpStatus | null>(null);
  const [loading, setLoading] = useState(false);
  const [portInput, setPortInput] = useState("3921");
  const [webhookLog, setWebhookLog] = useState<WebhookLogEntry[]>([]);
  const [showWebhookLog, setShowWebhookLog] = useState(false);

  const loadSettings = useCallback(async () => {
    try {
      const s = await getSettings();
      setSettings(s);
      setPortInput(String(s.mcpPort ?? 3921));
    } catch {
      // Settings not available yet
    }
  }, []);

  const fetchStatus = useCallback(async () => {
    try {
      const status = await invoke<McpStatus>("mcp_get_status");
      setMcpStatus(status);
    } catch {
      // MCP commands not available yet
    }
  }, []);

  const fetchWebhookLog = useCallback(async () => {
    try {
      const log = await invoke<WebhookLogEntry[]>("webhook_get_log");
      setWebhookLog(log.reverse()); // Most recent first
    } catch {
      // Webhook log not available
    }
  }, []);

  useEffect(() => {
    loadSettings();
    fetchStatus();
    fetchWebhookLog();
  }, [loadSettings, fetchStatus, fetchWebhookLog]);

  const handleToggle = async () => {
    if (!settings) return;
    setLoading(true);
    try {
      const newEnabled = !settings.mcpEnabled;
      const newSettings: Settings = {
        ...settings,
        mcpEnabled: newEnabled,
      };
      await saveSettings(newSettings);
      setSettings(newSettings);

      // Restart MCP server with new settings
      const status = await invoke<McpStatus>("mcp_restart");
      setMcpStatus(status);

      toast.success(
        newEnabled ? "MCP server started" : "MCP server stopped"
      );
    } catch (err) {
      console.error("Failed to toggle MCP:", err);
      toast.error("Failed to toggle MCP server");
    } finally {
      setLoading(false);
    }
  };

  const handlePortChange = async () => {
    if (!settings) return;
    const port = parseInt(portInput, 10);
    if (isNaN(port) || port < 1024 || port > 65535) {
      toast.error("Port must be between 1024 and 65535");
      setPortInput(String(settings.mcpPort ?? 3921));
      return;
    }

    setLoading(true);
    try {
      const newSettings: Settings = {
        ...settings,
        mcpPort: port,
      };
      await saveSettings(newSettings);
      setSettings(newSettings);

      // Restart if running
      if (settings.mcpEnabled) {
        const status = await invoke<McpStatus>("mcp_restart");
        setMcpStatus(status);
        toast.success(`MCP server restarted on port ${port}`);
      }
    } catch (err) {
      console.error("Failed to update port:", err);
      toast.error("Failed to update port");
    } finally {
      setLoading(false);
    }
  };

  const copyToClipboard = async (text: string) => {
    try {
      await invoke("copy_to_clipboard", { text });
      toast.success("Copied to clipboard");
    } catch {
      toast.error("Failed to copy");
    }
  };

  const port = settings?.mcpPort ?? 3921;

  const claudeCodeConfig = JSON.stringify(
    {
      mcpServers: {
        scratch: {
          command: "node",
          args: [
            "/Applications/Scratch.app/Contents/Resources/scratch-mcp-stdio.mjs",
          ],
        },
      },
    },
    null,
    2
  );

  const claudeDesktopConfig = JSON.stringify(
    {
      mcpServers: {
        scratch: {
          url: `http://localhost:${port}/mcp`,
        },
      },
    },
    null,
    2
  );

  return (
    <div className="space-y-8">
      {/* MCP Server */}
      <section>
        <h2 className="text-xl font-medium mb-0.5">MCP Server</h2>
        <p className="text-sm text-text-muted mb-4">
          Enable the MCP server to let Claude read, create, and edit your notes
          directly
        </p>

        <div className="rounded-[10px] border border-border p-4 space-y-4">
          {/* Enable toggle */}
          <div className="flex items-center justify-between">
            <div>
              <span className="text-sm font-medium">Enable MCP Server</span>
              <p className="text-xs text-text-muted mt-0.5">
                Serves MCP protocol on localhost:{port}
              </p>
            </div>
            <button
              onClick={handleToggle}
              disabled={loading}
              className={`relative w-10 h-[22px] rounded-full transition-colors cursor-pointer ${
                settings?.mcpEnabled
                  ? "bg-accent"
                  : "bg-bg-muted"
              }`}
            >
              <span
                className={`absolute top-[3px] w-4 h-4 rounded-full bg-white transition-transform shadow-sm ${
                  settings?.mcpEnabled
                    ? "translate-x-[21px]"
                    : "translate-x-[3px]"
                }`}
              />
            </button>
          </div>

          {/* Status */}
          <div className="flex items-center justify-between">
            <span className="text-sm text-text font-medium">Status</span>
            <div className="flex items-center gap-1.5">
              {loading ? (
                <SpinnerIcon className="w-3.5 h-3.5 animate-spin text-text-muted" />
              ) : (
                <span
                  className={`w-2 h-2 rounded-full ${
                    mcpStatus?.running ? "bg-green-500" : "bg-red-400"
                  }`}
                />
              )}
              <span className="text-sm text-text-muted">
                {mcpStatus?.running ? "Running" : "Stopped"}
              </span>
            </div>
          </div>

          {/* Port */}
          <div className="flex items-center justify-between gap-4">
            <span className="text-sm text-text font-medium">Port</span>
            <div className="flex items-center gap-2">
              <Input
                type="number"
                value={portInput}
                onChange={(e) => setPortInput(e.target.value)}
                onBlur={handlePortChange}
                onKeyDown={(e) => {
                  if (e.key === "Enter") handlePortChange();
                }}
                className="w-24 text-right"
                min={1024}
                max={65535}
              />
            </div>
          </div>
        </div>
      </section>

      {/* Divider */}
      <div className="border-t border-border border-dashed" />

      {/* Connection Instructions */}
      <section>
        <h2 className="text-xl font-medium mb-0.5">Connect Claude</h2>
        <p className="text-sm text-text-muted mb-4">
          Configure Claude to connect to your Scratch notes
        </p>

        {/* Claude Code */}
        <div className="space-y-4">
          <div>
            <h3 className="text-sm font-medium mb-1.5">Claude Code (CLI)</h3>
            <p className="text-xs text-text-muted mb-2">
              Add to ~/.claude/settings.json or project settings:
            </p>
            <div className="relative">
              <pre className="bg-bg-secondary rounded-lg border border-border p-3 text-xs font-mono overflow-x-auto text-text-muted">
                {claudeCodeConfig}
              </pre>
              <Button
                variant="ghost"
                size="sm"
                className="absolute top-2 right-2 h-7 w-7 p-0"
                onClick={() => copyToClipboard(claudeCodeConfig)}
                title="Copy"
              >
                <CopyIcon className="w-3.5 h-3.5" />
              </Button>
            </div>
          </div>

          {/* Claude Desktop */}
          <div>
            <h3 className="text-sm font-medium mb-1.5">Claude Desktop</h3>
            <p className="text-xs text-text-muted mb-2">
              Add to Claude Desktop's MCP server configuration:
            </p>
            <div className="relative">
              <pre className="bg-bg-secondary rounded-lg border border-border p-3 text-xs font-mono overflow-x-auto text-text-muted">
                {claudeDesktopConfig}
              </pre>
              <Button
                variant="ghost"
                size="sm"
                className="absolute top-2 right-2 h-7 w-7 p-0"
                onClick={() => copyToClipboard(claudeDesktopConfig)}
                title="Copy"
              >
                <CopyIcon className="w-3.5 h-3.5" />
              </Button>
            </div>
          </div>

          {/* Claude Web */}
          <div>
            <h3 className="text-sm font-medium mb-1.5">Claude Web</h3>
            <p className="text-xs text-text-muted mb-2">
              Use a tunnel to expose your local MCP server:
            </p>
            <div className="relative">
              <pre className="bg-bg-secondary rounded-lg border border-border p-3 text-xs font-mono overflow-x-auto text-text-muted">
                npx cloudflared tunnel --url http://localhost:{port}
              </pre>
              <Button
                variant="ghost"
                size="sm"
                className="absolute top-2 right-2 h-7 w-7 p-0"
                onClick={() =>
                  copyToClipboard(
                    `npx cloudflared tunnel --url http://localhost:${port}`
                  )
                }
                title="Copy"
              >
                <CopyIcon className="w-3.5 h-3.5" />
              </Button>
            </div>
            <p className="text-xs text-text-muted mt-1.5">
              Then add the tunnel URL as a Custom Connector in Claude Web
              settings (requires paid plan).
            </p>
          </div>
        </div>
      </section>

      {/* Divider */}
      <div className="border-t border-border border-dashed" />

      {/* Webhook Activity Log */}
      <section>
        <div className="flex items-center justify-between mb-0.5">
          <h2 className="text-xl font-medium">Webhook Activity</h2>
          <Button
            variant="ghost"
            size="sm"
            onClick={() => {
              setShowWebhookLog(!showWebhookLog);
              if (!showWebhookLog) fetchWebhookLog();
            }}
          >
            {showWebhookLog ? "Hide" : "Show"} Log
          </Button>
        </div>
        <p className="text-sm text-text-muted mb-4">
          Recent webhook events received by the MCP server
        </p>

        {showWebhookLog && (
          <div className="rounded-[10px] border border-border overflow-hidden">
            {webhookLog.length === 0 ? (
              <div className="p-4 text-sm text-text-muted text-center">
                No webhook events recorded yet
              </div>
            ) : (
              <div className="max-h-64 overflow-auto">
                <table className="w-full text-xs">
                  <thead className="bg-bg-secondary sticky top-0">
                    <tr>
                      <th className="text-left px-3 py-2 font-medium text-text-muted">Time</th>
                      <th className="text-left px-3 py-2 font-medium text-text-muted">Plugin</th>
                      <th className="text-left px-3 py-2 font-medium text-text-muted">Event</th>
                      <th className="text-left px-3 py-2 font-medium text-text-muted">Action</th>
                      <th className="text-left px-3 py-2 font-medium text-text-muted">Status</th>
                    </tr>
                  </thead>
                  <tbody>
                    {webhookLog.map((entry, i) => (
                      <tr
                        key={i}
                        className="border-t border-border"
                        title={entry.error ?? entry.note_id ?? ""}
                      >
                        <td className="px-3 py-1.5 text-text-muted whitespace-nowrap">
                          {entry.timestamp.replace("T", " ").replace("Z", "")}
                        </td>
                        <td className="px-3 py-1.5 font-mono">{entry.plugin}</td>
                        <td className="px-3 py-1.5 font-mono">{entry.event}</td>
                        <td className="px-3 py-1.5">{entry.action}</td>
                        <td className="px-3 py-1.5">
                          <span
                            className={`inline-flex items-center gap-1 ${
                              entry.success
                                ? "text-green-600 dark:text-green-400"
                                : "text-red-600 dark:text-red-400"
                            }`}
                          >
                            <span
                              className={`w-1.5 h-1.5 rounded-full ${
                                entry.success ? "bg-green-500" : "bg-red-400"
                              }`}
                            />
                            {entry.success ? "OK" : "Error"}
                          </span>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </div>
        )}
      </section>
    </div>
  );
}
