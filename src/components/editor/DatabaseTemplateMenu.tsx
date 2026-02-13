import { useState, useEffect, useRef, useCallback } from "react";
import * as db from "../../services/database";
import type { RowTemplateInfo, DatabaseRow } from "../../types/database";

/**
 * Reusable "New from template" dropdown menu for database views.
 *
 * Shows a list of available row templates for a database.
 * When a template with {{title}} is selected, prompts for the title.
 * When a template without variables is selected, creates the row immediately.
 */
export function DatabaseTemplateMenu({
  dbId,
  onRowCreated,
  className,
}: {
  dbId: string;
  onRowCreated: (row: DatabaseRow) => void;
  className?: string;
}) {
  const [open, setOpen] = useState(false);
  const [templates, setTemplates] = useState<RowTemplateInfo[]>([]);
  const [loading, setLoading] = useState(false);
  const [titlePrompt, setTitlePrompt] = useState<{
    templateName: string;
    templateDisplayName: string;
  } | null>(null);
  const [titleValue, setTitleValue] = useState("");
  const [creating, setCreating] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);
  const titleInputRef = useRef<HTMLInputElement>(null);

  // Load templates when menu opens
  useEffect(() => {
    if (!open) return;
    setLoading(true);
    db.listRowTemplates(dbId)
      .then(setTemplates)
      .catch(() => setTemplates([]))
      .finally(() => setLoading(false));
  }, [open, dbId]);

  // Close on outside click
  useEffect(() => {
    if (!open) return;
    const handleClickOutside = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        setOpen(false);
        setTitlePrompt(null);
      }
    };
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, [open]);

  // Focus title input when prompt appears
  useEffect(() => {
    if (titlePrompt) {
      titleInputRef.current?.focus();
    }
  }, [titlePrompt]);

  const handleSelectTemplate = useCallback(
    async (templateInfo: RowTemplateInfo) => {
      // Check if the template needs a title variable
      const needsTitle =
        templateInfo.title && templateInfo.title.includes("{{title}}");

      if (needsTitle) {
        setTitlePrompt({
          templateName: templateInfo.id,
          templateDisplayName: templateInfo.name,
        });
        setTitleValue("");
        return;
      }

      // No variables needed — create immediately
      setCreating(true);
      try {
        const newRow = await db.createRowFromTemplate(
          dbId,
          templateInfo.id,
          {}
        );
        onRowCreated(newRow);
        setOpen(false);
      } catch (err) {
        console.error("Failed to create row from template:", err);
      } finally {
        setCreating(false);
      }
    },
    [dbId, onRowCreated]
  );

  const handleCreateWithTitle = useCallback(async () => {
    if (!titlePrompt || !titleValue.trim()) return;

    setCreating(true);
    try {
      const newRow = await db.createRowFromTemplate(
        dbId,
        titlePrompt.templateName,
        { title: titleValue.trim() }
      );
      onRowCreated(newRow);
      setOpen(false);
      setTitlePrompt(null);
      setTitleValue("");
    } catch (err) {
      console.error("Failed to create row from template:", err);
    } finally {
      setCreating(false);
    }
  }, [dbId, titlePrompt, titleValue, onRowCreated]);

  // Don't render button if there are no templates (checked after first load)
  // We still show the button initially and load lazily
  if (templates.length === 0 && !open && !loading) {
    // On first render we haven't loaded yet, so show the button.
    // After loading with 0 templates, we hide.
    // Use a ref to track if we've loaded at least once.
  }

  return (
    <div
      ref={menuRef}
      className={`db-template-menu-wrapper ${className || ""}`}
      style={{ position: "relative", display: "inline-block" }}
    >
      <button
        type="button"
        className="db-template-menu-trigger"
        onClick={() => setOpen(!open)}
        title="New row from template"
      >
        <TemplateIcon />
        <span>From Template</span>
      </button>

      {open && (
        <div className="db-template-menu-dropdown">
          {loading ? (
            <div className="db-template-menu-loading">Loading...</div>
          ) : templates.length === 0 ? (
            <div className="db-template-menu-empty">
              No templates defined.
              <span className="db-template-menu-hint">
                Add templates to the database schema.
              </span>
            </div>
          ) : titlePrompt ? (
            <div className="db-template-menu-title-prompt">
              <div className="db-template-menu-title-label">
                {titlePrompt.templateDisplayName}
              </div>
              <input
                ref={titleInputRef}
                type="text"
                className="db-template-menu-title-input"
                placeholder="Enter title..."
                value={titleValue}
                onChange={(e) => setTitleValue(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    e.preventDefault();
                    handleCreateWithTitle();
                  }
                  if (e.key === "Escape") {
                    e.preventDefault();
                    setTitlePrompt(null);
                  }
                  e.stopPropagation();
                }}
                disabled={creating}
              />
              <div className="db-template-menu-title-actions">
                <button
                  type="button"
                  className="db-template-menu-title-cancel"
                  onClick={() => setTitlePrompt(null)}
                  disabled={creating}
                >
                  Cancel
                </button>
                <button
                  type="button"
                  className="db-template-menu-title-create"
                  onClick={handleCreateWithTitle}
                  disabled={creating || !titleValue.trim()}
                >
                  {creating ? "Creating..." : "Create"}
                </button>
              </div>
            </div>
          ) : (
            <div className="db-template-menu-list">
              {templates.map((tmpl) => (
                <button
                  key={tmpl.id}
                  type="button"
                  className="db-template-menu-item"
                  onClick={() => handleSelectTemplate(tmpl)}
                  disabled={creating}
                >
                  <span className="db-template-menu-item-name">
                    {tmpl.name}
                  </span>
                  {tmpl.title && (
                    <span className="db-template-menu-item-pattern">
                      {tmpl.title}
                    </span>
                  )}
                </button>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

function TemplateIcon() {
  return (
    <svg
      width="14"
      height="14"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <rect x="3" y="3" width="18" height="18" rx="2" ry="2" />
      <line x1="3" y1="9" x2="21" y2="9" />
      <line x1="9" y1="21" x2="9" y2="9" />
    </svg>
  );
}
