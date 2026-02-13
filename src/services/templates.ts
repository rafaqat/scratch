import { invoke } from "@tauri-apps/api/core";

export interface TemplateInfo {
  id: string;
  name: string;
  description: string;
  isBuiltin: boolean;
}

export interface TemplateNoteResult {
  note: {
    id: string;
    title: string;
    content: string;
    path: string;
    modified: number;
  };
  cursorLine: number | null;
}

export async function listTemplates(): Promise<TemplateInfo[]> {
  return invoke("list_templates");
}

export async function readTemplate(id: string): Promise<string> {
  return invoke("read_template", { id });
}

export async function createNoteFromTemplate(
  templateId: string,
  title?: string
): Promise<TemplateNoteResult> {
  return invoke("create_note_from_template", { templateId, title });
}
