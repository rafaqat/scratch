export interface NoteMetadata {
  id: string;
  title: string;
  preview: string;
  modified: number;
  icon?: string;
}

export interface Note {
  id: string;
  title: string;
  content: string;
  path: string;
  modified: number;
}

export interface ThemeSettings {
  mode: "light" | "dark" | "system";
}

export type FontFamily = "system-sans" | "serif" | "monospace";

export interface EditorFontSettings {
  baseFontFamily?: FontFamily;
  baseFontSize?: number; // in px, default 16
  boldWeight?: number; // 600, 700, 800 for headings and bold text
  lineHeight?: number; // default 1.6
}

// Per-folder settings (stored in .scratch/settings.json)
export interface Settings {
  theme: ThemeSettings;
  editorFont?: EditorFontSettings;
  gitEnabled?: boolean;
  pinnedNoteIds?: string[];
  mcpEnabled?: boolean;
  mcpPort?: number;
}

export interface McpStatus {
  running: boolean;
  port: number;
}

export interface PluginValidation {
  valid: boolean;
  errors: string[];
  warnings: string[];
}

export interface PluginInfo {
  name: string;
  version: string;
  description: string;
  enabled: boolean;
  tool_count: number;
  webhook_count: number;
  permissions: string[];
  validation: PluginValidation;
}

export interface WebhookLogEntry {
  timestamp: string;
  plugin: string;
  event: string;
  action: string;
  success: boolean;
  error?: string;
  note_id?: string;
}
