import { useEffect, useRef, useCallback, useState } from "react";
import { createPortal } from "react-dom";
import {
  useCreateBlockNote,
  SuggestionMenuController,
  getDefaultReactSlashMenuItems,
  LinkToolbarController,
  EditLinkButton,
  DeleteLinkButton,
  useComponentsContext,
  FormattingToolbarController,
  FormattingToolbar,
  BlockTypeSelect,
  BasicTextStyleButton,
  TextAlignButton,
  ColorStyleButton,
  NestBlockButton,
  UnnestBlockButton,
  useBlockNoteEditor,
} from "@blocknote/react";
import type { LinkToolbarProps } from "@blocknote/react";
import {
  filterSuggestionItems,
  insertOrUpdateBlockForSlashMenu,
} from "@blocknote/core/extensions";
import { BlockNoteView } from "@blocknote/mantine";
import { Link as TiptapLink } from "@tiptap/extension-link";
import { mergeAttributes } from "@tiptap/core";
import "@blocknote/core/fonts/inter.css";
import "@blocknote/mantine/style.css";
import "katex/dist/katex.min.css";
import { listen } from "@tauri-apps/api/event";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { join } from "@tauri-apps/api/path";
import { toast } from "sonner";
import { mod, shift, isMac } from "../../lib/platform";
import { parseFrontmatter, recombine, type StoryFrontmatter } from "../../lib/frontmatter";
import { parseNoteMeta, recombineNoteMeta, type NotePageMeta } from "../../lib/noteMeta";
import { EmojiPicker, getRandomEmoji } from "./EmojiPicker";
import { preprocessSvg, postprocessSvg } from "../../lib/svg";
import { preprocessCallouts, postprocessCallouts } from "../../lib/callout";
import { postprocessEquations } from "../../lib/equation";
import { preprocessBookmarks, postprocessBookmarks } from "../../lib/bookmark";
import { preprocessDatabaseRefs, postprocessDatabaseRefs } from "../../lib/databaseRef";
import { injectWikilinks } from "../../lib/wikilink";
import { getWikilinkMenuItems, updateNoteTitles } from "./Wikilink";
import { schema } from "./schema";
import { DatabaseIcon } from "./DatabaseTable";
import { StoryMetaCard } from "./StoryMetaCard";
import { BacklinksPanel } from "./BacklinksPanel";

import * as DropdownMenu from "@radix-ui/react-dropdown-menu";
import { useNotes } from "../../context/NotesContext";
import { useTheme } from "../../context/ThemeContext";
import { cn } from "../../lib/utils";
import { Button, IconButton, Tooltip } from "../ui";
import * as notesService from "../../services/notes";
import type { Settings } from "../../types/note";
import {
  SpinnerIcon,
  CircleCheckIcon,
  CopyIcon,
  PanelLeftIcon,
  RefreshCwIcon,
  PinIcon,
  ImageIcon,
  EditIcon,
  FolderIcon,
  LinkIcon,
  ArrowDownToLineIcon,
  ClockIcon,
  RotateCcwIcon,
  XIcon,
  ChevronRightIcon,
} from "../icons";
import type { NoteMetadata } from "../../types/note";

type ResolvedNoteLink = {
  noteId: string;
  attemptedId: string;
};

function resolvePath(currentId: string, relativePath: string) {
  const currentParts = currentId.split("/");
  currentParts.pop();
  for (const part of relativePath.split("/")) {
    if (part === "" || part === ".") continue;
    if (part === "..") {
      if (currentParts.length > 0) currentParts.pop();
    } else {
      currentParts.push(part);
    }
  }
  return currentParts.join("/");
}

function stripHashAndQuery(value: string) {
  return value.split("#")[0].split("?")[0] ?? "";
}

function tryDecode(value: string) {
  try {
    return decodeURIComponent(value);
  } catch {
    return value;
  }
}

function isExternalHref(value: string) {
  return (
    value.startsWith("http://") ||
    value.startsWith("https://") ||
    value.startsWith("mailto:")
  );
}

function isLocalhostUrl(value: string) {
  try {
    const parsed = new URL(value);
    const host = parsed.hostname.toLowerCase();
    return host === "localhost" || host.endsWith(".localhost");
  } catch {
    return false;
  }
}

function resolveNoteLink(
  href: string,
  currentNoteId: string | null,
  notes: NoteMetadata[],
): ResolvedNoteLink | null {
  const trimmed = href.trim();
  if (!trimmed || trimmed.startsWith("#") || trimmed.startsWith("view:")) {
    return null;
  }

  let pathCandidate = "";
  let absolute = false;

  if (/^[a-zA-Z][a-zA-Z\d+\-.]*:/.test(trimmed)) {
    if (!isLocalhostUrl(trimmed)) {
      return null;
    }
    const parsed = new URL(trimmed);
    pathCandidate = tryDecode(parsed.pathname || "");
    absolute = true;
  } else if (trimmed.startsWith("//")) {
    return null;
  } else {
    pathCandidate = tryDecode(trimmed);
    absolute = pathCandidate.startsWith("/");
  }

  let normalized = stripHashAndQuery(pathCandidate).trim();
  if (!normalized) {
    return null;
  }

  if (normalized.toLowerCase().endsWith(".md")) {
    normalized = normalized.slice(0, -3);
  }
  normalized = normalized.replace(/^\/+/, "");
  if (!normalized) {
    return null;
  }

  const noteId = absolute
    ? normalized
    : currentNoteId
      ? resolvePath(currentNoteId, normalized)
      : normalized;

  const exact = notes.find((n) => n.id === noteId);
  if (exact) {
    return { noteId: exact.id, attemptedId: noteId };
  }

  const needle = noteId.split("/").pop()?.toLowerCase();
  if (!needle) {
    return { noteId, attemptedId: noteId };
  }

  const fallback = notes.find((n) => {
    const byId = n.id.split("/").pop()?.toLowerCase() === needle;
    const byTitle = n.title.toLowerCase() === needle;
    return byId || byTitle;
  });
  if (fallback) {
    return { noteId: fallback.id, attemptedId: noteId };
  }

  return { noteId, attemptedId: noteId };
}

function openExternalLink(rawHref: string, resolvedHref: string) {
  const url = isExternalHref(rawHref) ? rawHref : resolvedHref;
  if (isExternalHref(url)) {
    invoke("open_url_safe", { url });
  }
}

/** Compute a relative path from one note to another (e.g. "../other/target.md") */
function computeRelativePath(fromId: string, toId: string): string {
  const fromParts = fromId.split("/");
  fromParts.pop(); // remove filename → directory parts
  const toParts = toId.split("/");

  // Find common prefix length
  let common = 0;
  while (common < fromParts.length && common < toParts.length - 1 && fromParts[common] === toParts[common]) {
    common++;
  }

  const ups = fromParts.length - common;
  const remaining = toParts.slice(common);

  const parts = [...Array(ups).fill(".."), ...remaining];
  const result = parts.join("/");
  // Ensure it ends with .md
  return result.endsWith(".md") ? result : result + ".md";
}

/** Searchable note picker with Obsidian-style folder navigation */
function NotePickerPopover({
  onSelect,
  onClose,
  notes,
  anchorRect,
}: {
  onSelect: (noteId: string, noteTitle: string) => void;
  onClose: () => void;
  notes: NoteMetadata[];
  anchorRect: DOMRect | null;
}) {
  const [query, setQuery] = useState("");
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [searchResults, setSearchResults] = useState<NoteMetadata[]>([]);
  // Folder navigation state
  const [currentFolder, setCurrentFolder] = useState<string | null>(null);
  const [folders, setFolders] = useState<string[]>([]);
  const [folderNotes, setFolderNotes] = useState<NoteMetadata[]>([]);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  const isSearching = query.trim().length > 0;

  // Load folder contents when currentFolder changes
  useEffect(() => {
    const loadFolder = async () => {
      try {
        const [foldersResult, notesResult] = await Promise.all([
          invoke<string[]>("list_folders", { parent: currentFolder }),
          invoke<NoteMetadata[]>("list_notes_in_folder", { folder: currentFolder }),
        ]);
        setFolders(foldersResult);
        setFolderNotes(notesResult);
      } catch {
        setFolders([]);
        setFolderNotes([]);
      }
    };
    loadFolder();
  }, [currentFolder]);

  // Auto-focus input
  useEffect(() => {
    setTimeout(() => inputRef.current?.focus(), 50);
  }, []);

  // Debounced Tantivy search (global, across all folders)
  useEffect(() => {
    const trimmed = query.trim();
    if (!trimmed) {
      setSearchResults([]);
      return;
    }
    const timer = setTimeout(async () => {
      try {
        const results = await invoke<NoteMetadata[]>("search_notes", { query: trimmed });
        setSearchResults(results);
      } catch {
        // ignore
      }
    }, 150);
    return () => clearTimeout(timer);
  }, [query]);

  // Build the display list: in search mode use search results, in browse mode show folders + notes
  type ListItem = { kind: "back" } | { kind: "folder"; path: string; name: string } | { kind: "note"; note: NoteMetadata };
  const items: ListItem[] = [];

  if (isSearching) {
    // Search mode: show global results with path context
    const results = searchResults.length > 0
      ? searchResults.slice(0, 15)
      : notes.filter((n) => n.title.toLowerCase().includes(query.toLowerCase())).slice(0, 15);
    for (const note of results) {
      items.push({ kind: "note", note });
    }
  } else {
    // Browse mode
    if (currentFolder !== null) {
      items.push({ kind: "back" });
    }
    for (const folderPath of folders) {
      const name = folderPath.includes("/") ? folderPath.split("/").pop()! : folderPath;
      items.push({ kind: "folder", path: folderPath, name });
    }
    for (const note of folderNotes) {
      items.push({ kind: "note", note });
    }
  }

  // Reset selection on query or folder change
  useEffect(() => { setSelectedIndex(0); }, [query, currentFolder]);

  // Scroll selected into view
  useEffect(() => {
    const item = listRef.current?.querySelector(`[data-idx="${selectedIndex}"]`);
    item?.scrollIntoView({ block: "nearest" });
  }, [selectedIndex]);

  const navigateUp = () => {
    if (currentFolder === null) return;
    const parts = currentFolder.split("/");
    parts.pop();
    setCurrentFolder(parts.length > 0 ? parts.join("/") : null);
  };

  const handleItemAction = (idx: number) => {
    const item = items[idx];
    if (!item) return;
    if (item.kind === "back") {
      navigateUp();
    } else if (item.kind === "folder") {
      setCurrentFolder(item.path);
      setQuery("");
    } else {
      onSelect(item.note.id, item.note.title);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    switch (e.key) {
      case "ArrowDown":
        e.preventDefault();
        setSelectedIndex((i) => Math.min(i + 1, items.length - 1));
        break;
      case "ArrowUp":
        e.preventDefault();
        setSelectedIndex((i) => Math.max(i - 1, 0));
        break;
      case "Enter":
        e.preventDefault();
        handleItemAction(selectedIndex);
        break;
      case "Backspace":
        if (query === "" && currentFolder !== null) {
          e.preventDefault();
          navigateUp();
        }
        break;
      case "Escape":
        e.preventDefault();
        onClose();
        break;
    }
  };

  // Breadcrumb segments
  const breadcrumbs = currentFolder ? currentFolder.split("/") : [];

  // Get folder path for a note (everything before the last /)
  const noteFolderPath = (noteId: string) => {
    const idx = noteId.lastIndexOf("/");
    return idx > 0 ? noteId.substring(0, idx) : "";
  };

  // Position below the anchor
  const style: React.CSSProperties = anchorRect
    ? { position: "fixed", top: anchorRect.bottom + 4, left: Math.max(8, anchorRect.left - 100), zIndex: 9999 }
    : { position: "fixed", top: "50%", left: "50%", transform: "translate(-50%, -50%)", zIndex: 9999 };

  return createPortal(
    <>
      <div className="fixed inset-0 z-[9998]" onClick={onClose} />
      <div style={style} className="w-80 bg-bg border border-border rounded-lg shadow-xl overflow-hidden">
        {/* Breadcrumb bar (browse mode only) */}
        {!isSearching && (
          <div className="flex items-center gap-0.5 px-2.5 pt-2 pb-1 text-xs text-text-muted overflow-x-auto">
            <button
              onClick={() => setCurrentFolder(null)}
              className={cn(
                "shrink-0 hover:text-text transition-colors",
                currentFolder === null && "text-text font-medium",
              )}
            >
              Root
            </button>
            {breadcrumbs.map((seg, i) => {
              const path = breadcrumbs.slice(0, i + 1).join("/");
              const isLast = i === breadcrumbs.length - 1;
              return (
                <span key={path} className="flex items-center gap-0.5 shrink-0">
                  <span className="text-text-muted/40">/</span>
                  <button
                    onClick={() => setCurrentFolder(path)}
                    className={cn(
                      "hover:text-text transition-colors",
                      isLast && "text-text font-medium",
                    )}
                  >
                    {seg}
                  </button>
                </span>
              );
            })}
          </div>
        )}
        {/* Search input */}
        <div className="px-2 py-1.5">
          <input
            ref={inputRef}
            type="text"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder={isSearching ? "Search all notes..." : "Type to search or browse folders..."}
            autoComplete="off"
            autoCorrect="off"
            spellCheck={false}
            className="w-full px-2.5 py-1.5 text-sm bg-bg-muted rounded-md outline-none text-text placeholder-text-muted/50"
          />
        </div>
        {/* Items list */}
        <div ref={listRef} className="max-h-60 overflow-y-auto p-1 border-t border-border">
          {items.length === 0 ? (
            <div className="px-3 py-2 text-xs text-text-muted">
              {isSearching ? "No notes found" : "Empty folder"}
            </div>
          ) : (
            items.map((item, i) => {
              if (item.kind === "back") {
                return (
                  <button
                    key="__back"
                    data-idx={i}
                    onClick={() => navigateUp()}
                    className={cn(
                      "w-full text-left px-2.5 py-1.5 text-sm rounded-md flex items-center gap-2",
                      i === selectedIndex ? "bg-accent text-white" : "text-text-muted hover:bg-bg-muted",
                    )}
                  >
                    <span className="text-xs">←</span>
                    <span>..</span>
                  </button>
                );
              }
              if (item.kind === "folder") {
                return (
                  <button
                    key={`folder:${item.path}`}
                    data-idx={i}
                    onClick={() => { setCurrentFolder(item.path); setQuery(""); }}
                    className={cn(
                      "w-full text-left px-2.5 py-1.5 text-sm rounded-md flex items-center gap-2",
                      i === selectedIndex ? "bg-accent text-white" : "text-text hover:bg-bg-muted",
                    )}
                  >
                    <FolderIcon className="w-3.5 h-3.5 shrink-0 opacity-60" />
                    <span className="truncate">{item.name}</span>
                  </button>
                );
              }
              // Note item
              const folderPath = noteFolderPath(item.note.id);
              return (
                <button
                  key={item.note.id}
                  data-idx={i}
                  onClick={() => onSelect(item.note.id, item.note.title)}
                  className={cn(
                    "w-full text-left px-2.5 py-1.5 text-sm rounded-md flex flex-col",
                    i === selectedIndex ? "bg-accent text-white" : "text-text hover:bg-bg-muted",
                  )}
                >
                  <span className="truncate">{item.note.title}</span>
                  {isSearching && folderPath && (
                    <span className={cn(
                      "text-xs truncate",
                      i === selectedIndex ? "text-white/60" : "text-text-muted/60",
                    )}>
                      {folderPath}
                    </span>
                  )}
                </button>
              );
            })
          )}
        </div>
      </div>
    </>,
    document.body,
  );
}

/**
 * Custom "Create Link" button for the formatting toolbar.
 * Replaces BlockNote's default CreateLinkButton to show a note picker
 * instead of a plain URL input. Notion-style: select text → Cmd+K → pick a note.
 */
function CustomCreateLinkButton({
  notes,
  currentNoteIdRef,
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  editorInstance,
}: {
  notes: NoteMetadata[];
  currentNoteIdRef: React.RefObject<string | null>;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  editorInstance: any;
}) {
  const editor = useBlockNoteEditor();
  const Components = useComponentsContext()!;
  const [pickerOpen, setPickerOpen] = useState(false);
  const btnRef = useRef<HTMLDivElement>(null);
  // Save the editor selection before focus moves to the picker
  const savedSelectionRef = useRef<{ from: number; to: number } | null>(null);

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const getTiptap = () => (editorInstance as any)?._tiptapEditor;

  const openPicker = () => {
    // Snapshot the current selection before the picker steals focus
    const tiptap = getTiptap();
    if (tiptap) {
      const { from, to } = tiptap.state.selection;
      savedSelectionRef.current = { from, to };
      console.log("[CustomCreateLink] Saved selection", { from, to });
    }
    setPickerOpen(true);
  };

  // Cmd+K handler on editor DOM element (same level as BlockNote's default)
  useEffect(() => {
    const domEl = editor.domElement;
    if (!domEl) return;

    const handleKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && !e.shiftKey && !e.altKey && e.key.toLowerCase() === "k") {
        console.log("[CustomCreateLink] Cmd+K intercepted");
        e.preventDefault();
        e.stopPropagation();
        e.stopImmediatePropagation();
        openPicker();
      }
    };
    // Use capture to beat BlockNote's handler
    domEl.addEventListener("keydown", handleKey, true);
    return () => domEl.removeEventListener("keydown", handleKey, true);
  }, [editor.domElement]);

  const handleNotePicked = (noteId: string, noteTitle: string) => {
    setPickerOpen(false);
    const currentId = currentNoteIdRef.current;
    const relativePath = currentId
      ? computeRelativePath(currentId, noteId)
      : noteId.endsWith(".md") ? noteId : noteId + ".md";

    console.log("[CustomCreateLink] Picked note:", noteId, "→", relativePath);

    const tiptap = getTiptap();
    if (!tiptap) return;

    const sel = savedSelectionRef.current;
    savedSelectionRef.current = null;

    // Use BlockNote's transact() with our saved positions
    editorInstance.transact((tr: any) => {
      const pmSchema = tr.doc.type.schema;
      const linkType = pmSchema.marks.link;
      if (!linkType) return;

      const mark = linkType.create({ href: relativePath });

      if (sel && sel.from !== sel.to) {
        console.log("[CustomCreateLink] Adding link mark to range", sel);
        tr.addMark(sel.from, sel.to, mark);
      } else {
        console.log("[CustomCreateLink] Inserting linked text:", noteTitle);
        const pos = sel?.from ?? tr.selection.from;
        const textNode = pmSchema.text(noteTitle, [mark]);
        tr.insert(pos, textNode);
      }
    });
    tiptap.view.focus();
  };

  return (
    <div ref={btnRef} style={{ display: "inline-flex" }}>
      <Components.FormattingToolbar.Button
        className="bn-button"
        data-test="createLink"
        label="Link to note"
        mainTooltip="Link to note (⌘K)"
        icon={<FolderIcon className="w-3.5 h-3.5" />}
        onClick={() => {
          console.log("[CustomCreateLink] Button clicked");
          openPicker();
        }}
      />
      {pickerOpen && (
        <NotePickerPopover
          notes={notes}
          anchorRect={btnRef.current?.getBoundingClientRect() ?? null}
          onSelect={handleNotePicked}
          onClose={() => setPickerOpen(false)}
        />
      )}
    </div>
  );
}

/**
 * Custom "URL Link" button for the formatting toolbar.
 * Opens a small popover to type/paste an https URL and apply it as a link.
 */
function CustomUrlLinkButton({
  editorInstance,
}: {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  editorInstance: any;
}) {
  const Components = useComponentsContext()!;
  const [open, setOpen] = useState(false);
  const [url, setUrl] = useState("");
  const btnRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const savedSelectionRef = useRef<{ from: number; to: number } | null>(null);

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const getTiptap = () => (editorInstance as any)?._tiptapEditor;

  const openPopover = () => {
    const tiptap = getTiptap();
    if (tiptap) {
      const { from, to } = tiptap.state.selection;
      savedSelectionRef.current = { from, to };
    }
    setUrl("");
    setOpen(true);
    setTimeout(() => inputRef.current?.focus(), 50);
  };

  const applyLink = () => {
    const trimmed = url.trim();
    if (!trimmed) return;

    // Ensure protocol
    const href = /^https?:\/\//i.test(trimmed) ? trimmed : `https://${trimmed}`;

    const tiptap = getTiptap();
    if (!tiptap) return;

    const sel = savedSelectionRef.current;
    savedSelectionRef.current = null;

    editorInstance.transact((tr: any) => {
      const pmSchema = tr.doc.type.schema;
      const linkType = pmSchema.marks.link;
      if (!linkType) return;

      const mark = linkType.create({ href });

      if (sel && sel.from !== sel.to) {
        tr.addMark(sel.from, sel.to, mark);
      } else {
        // No selection — insert the URL as link text
        const pos = sel?.from ?? tr.selection.from;
        const textNode = pmSchema.text(trimmed, [mark]);
        tr.insert(pos, textNode);
      }
    });

    setOpen(false);
    tiptap.view.focus();
  };

  const rect = btnRef.current?.getBoundingClientRect();
  const popoverStyle: React.CSSProperties = rect
    ? { position: "fixed", top: rect.bottom + 4, left: rect.left, zIndex: 9999 }
    : { position: "fixed", top: "50%", left: "50%", transform: "translate(-50%, -50%)", zIndex: 9999 };

  return (
    <div ref={btnRef} style={{ display: "inline-flex" }}>
      <Components.FormattingToolbar.Button
        className="bn-button"
        data-test="createUrlLink"
        label="Link to URL"
        mainTooltip="Link to URL"
        icon={<LinkIcon className="w-3.5 h-3.5" />}
        onClick={openPopover}
      />
      {open &&
        createPortal(
          <>
            <div className="fixed inset-0 z-[9998]" onClick={() => setOpen(false)} />
            <div style={popoverStyle} className="w-72 bg-bg border border-border rounded-lg shadow-xl overflow-hidden p-2">
              <input
                ref={inputRef}
                type="text"
                value={url}
                onChange={(e) => setUrl(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    e.preventDefault();
                    applyLink();
                  }
                  if (e.key === "Escape") {
                    e.preventDefault();
                    setOpen(false);
                  }
                }}
                placeholder="Paste or type a URL..."
                autoComplete="off"
                autoCorrect="off"
                spellCheck={false}
                className="w-full px-2.5 py-1.5 text-sm bg-bg-muted rounded-md outline-none text-text placeholder-text-muted/50"
              />
            </div>
          </>,
          document.body,
        )}
    </div>
  );
}

// Custom link toolbar that routes links through Tauri instead of window.open
function CustomLinkToolbar(
  props: LinkToolbarProps & {
    notes: NoteMetadata[];
    selectNote: (id: string) => void;
    currentNoteId: string | null;
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    editor: any;
  },
) {
  const Components = useComponentsContext()!;
  const { notes, selectNote, currentNoteId, editor, ...toolbarProps } = props;
  const [pickerOpen, setPickerOpen] = useState(false);
  const pickerBtnRef = useRef<HTMLDivElement>(null);

  const handleOpen = () => {
    const href = toolbarProps.url;
    const resolved = resolveNoteLink(href, currentNoteId, notes);

    if (resolved && notes.some((n) => n.id === resolved.noteId)) {
      selectNote(resolved.noteId);
      return;
    }

    if (href.startsWith("#") || (isLocalhostUrl(href) && href.includes("#"))) {
      return;
    }

    if (resolved) {
      toast.error(`Note "${resolved.attemptedId}" not found`);
      return;
    }

    openExternalLink(href, href);
  };

  const handleNotePicked = (noteId: string, noteTitle: string) => {
    setPickerOpen(false);
    // Compute relative path from current note to picked note
    const relativePath = currentNoteId
      ? computeRelativePath(currentNoteId, noteId)
      : noteId.endsWith(".md") ? noteId : noteId + ".md";

    // Update the link via TipTap
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const tiptap = (editor as any)?._tiptapEditor;
    if (tiptap) {
      tiptap.chain().focus().extendMarkRange("link").updateAttributes("link", { href: relativePath }).run();
      // If the link text is just a URL, replace it with the note title
      const currentText = toolbarProps.text;
      if (!currentText || currentText === toolbarProps.url || currentText.startsWith("http")) {
        tiptap.chain().focus().extendMarkRange("link").insertContent(noteTitle).run();
      }
    }
    toolbarProps.setToolbarOpen?.(false);
  };

  return (
    <>
      <Components.LinkToolbar.Root className="bn-toolbar bn-link-toolbar">
        <EditLinkButton
          url={toolbarProps.url}
          text={toolbarProps.text}
          range={toolbarProps.range}
          setToolbarOpen={toolbarProps.setToolbarOpen}
          setToolbarPositionFrozen={toolbarProps.setToolbarPositionFrozen}
        />
        <div ref={pickerBtnRef}>
          <Components.LinkToolbar.Button
            className="bn-button"
            mainTooltip="Link to note"
            label="Link to note"
            isSelected={pickerOpen}
            onClick={() => {
              toolbarProps.setToolbarPositionFrozen?.(true);
              setPickerOpen((v) => !v);
            }}
            icon={<FolderIcon className="w-3.5 h-3.5" />}
          />
        </div>
        <Components.LinkToolbar.Button
          className="bn-button"
          mainTooltip="Open link"
          label="Open link"
          isSelected={false}
          onClick={handleOpen}
          icon={<span style={{ fontSize: 14 }}>&#x2197;</span>}
        />
        <DeleteLinkButton
          range={toolbarProps.range}
          setToolbarOpen={toolbarProps.setToolbarOpen}
        />
      </Components.LinkToolbar.Root>
      {pickerOpen && (
        <NotePickerPopover
          notes={notes}
          anchorRect={pickerBtnRef.current?.getBoundingClientRect() ?? null}
          onSelect={handleNotePicked}
          onClose={() => {
            setPickerOpen(false);
            toolbarProps.setToolbarPositionFrozen?.(false);
          }}
        />
      )}
    </>
  );
}

function formatDateTime(timestamp: number): string {
  const date = new Date(timestamp * 1000);
  return date.toLocaleDateString(undefined, {
    weekday: "short",
    month: "short",
    day: "numeric",
    year: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
}

// TOC icon for slash menu (Lucide list-tree)
const TocIcon = () => (
  <svg
    width="18"
    height="18"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth={2}
    strokeLinecap="round"
    strokeLinejoin="round"
  >
    <path d="M21 12h-8" />
    <path d="M21 6H8" />
    <path d="M21 18h-8" />
    <path d="M3 6v4c0 1.1.9 2 2 2h3" />
    <path d="M3 10v6c0 1.1.9 2 2 2h3" />
  </svg>
);

// Callout icon for slash menu (Lucide message-square-text)
const CalloutIcon = () => (
  <svg
    width="18"
    height="18"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth={2}
    strokeLinecap="round"
    strokeLinejoin="round"
  >
    <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" />
    <path d="M13 8H7" />
    <path d="M17 12H7" />
  </svg>
);

// Equation icon for slash menu (sigma symbol)
const EquationIcon = () => (
  <svg
    width="18"
    height="18"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth={2}
    strokeLinecap="round"
    strokeLinejoin="round"
  >
    <path d="M18 7V4H6l6 8-6 8h12v-3" />
  </svg>
);

// Bookmark icon for slash menu (Lucide bookmark / link)
const BookmarkIcon = () => (
  <svg
    width="18"
    height="18"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth={2}
    strokeLinecap="round"
    strokeLinejoin="round"
  >
    <path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71" />
    <path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71" />
  </svg>
);

// Toggle icon for slash menu (Lucide chevron-right in triangle)
const ToggleIcon = () => (
  <svg
    width="18"
    height="18"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth={2}
    strokeLinecap="round"
    strokeLinejoin="round"
  >
    <path d="M9 18l6-6-6-6" />
  </svg>
);

// Divider icon for slash menu (Lucide minus)
const DividerIcon = () => (
  <svg
    width="18"
    height="18"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth={2}
    strokeLinecap="round"
    strokeLinejoin="round"
  >
    <path d="M5 12h14" />
  </svg>
);

const ColumnsIcon = () => (
  <svg
    width="18"
    height="18"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth={2}
    strokeLinecap="round"
    strokeLinejoin="round"
  >
    <rect x="3" y="3" width="7" height="18" rx="1" />
    <rect x="14" y="3" width="7" height="18" rx="1" />
  </svg>
);

/**
 * Build custom slash menu items with the default items plus TOC, Callout, Equation, Bookmark, Divider, and Columns.
 */
function getCustomSlashMenuItems(
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  editor: any,
) {
  return [
    ...getDefaultReactSlashMenuItems(editor),
    {
      title: "Callout",
      onItemClick: () => {
        insertOrUpdateBlockForSlashMenu(editor, {
          type: "callout",
          props: { type: "info" },
        } as never);
      },
      aliases: ["callout", "alert", "admonition", "note", "warning", "tip"],
      group: "Basic blocks",
      icon: <CalloutIcon />,
      subtext: "Colored callout box for notes, tips, and warnings",
    },
    {
      title: "Equation",
      onItemClick: () => {
        insertOrUpdateBlockForSlashMenu(editor, {
          type: "equation" as never,
          props: { equation: "" } as never,
        });
      },
      aliases: ["equation", "math", "latex", "formula", "katex"],
      group: "Advanced",
      icon: <EquationIcon />,
      subtext: "Display math equation with LaTeX",
    },
    {
      title: "Bookmark",
      onItemClick: () => {
        insertOrUpdateBlockForSlashMenu(editor, {
          type: "bookmark",
          props: { url: "" },
        } as never);
      },
      aliases: ["bookmark", "link", "embed", "url", "web"],
      group: "Advanced",
      icon: <BookmarkIcon />,
      subtext: "URL preview card with title and description",
    },
    {
      title: "Database",
      onItemClick: () => {
        insertOrUpdateBlockForSlashMenu(editor, {
          type: "databaseTable" as never,
          props: { databaseName: "" } as never,
        });
      },
      aliases: ["database", "table view", "database table", "db"],
      group: "Advanced",
      icon: <DatabaseIcon />,
      subtext: "Editable table view of a database",
    },
    {
      title: "Toggle",
      onItemClick: () => {
        insertOrUpdateBlockForSlashMenu(editor, { type: "toggle" as never });
      },
      aliases: ["toggle", "collapse", "details", "disclosure", "accordion"],
      group: "Basic blocks",
      icon: <ToggleIcon />,
      subtext: "Collapsible toggle section",
    },
    {
      title: "Divider",
      onItemClick: () => {
        insertOrUpdateBlockForSlashMenu(editor, { type: "divider" as never });
      },
      aliases: ["divider", "hr", "separator", "line", "horizontal rule", "---"],
      group: "Basic blocks",
      icon: <DividerIcon />,
      subtext: "Horizontal line to separate content",
    },
    {
      title: "2 Columns",
      onItemClick: () => {
        insertOrUpdateBlockForSlashMenu(editor, {
          type: "columns" as never,
          props: { columnData: '["",""]' } as never,
        });
      },
      aliases: ["columns", "2-columns", "two columns", "side by side"],
      group: "Basic blocks",
      icon: <ColumnsIcon />,
      subtext: "Two-column side-by-side layout",
    },
    {
      title: "3 Columns",
      onItemClick: () => {
        insertOrUpdateBlockForSlashMenu(editor, {
          type: "columns" as never,
          props: { columnData: '["","",""]' } as never,
        });
      },
      aliases: ["3-columns", "three columns"],
      group: "Basic blocks",
      icon: <ColumnsIcon />,
      subtext: "Three-column layout",
    },
    {
      title: "Table of Contents",
      onItemClick: () => {
        insertOrUpdateBlockForSlashMenu(editor, { type: "toc" as never });
      },
      aliases: ["toc", "table of contents", "contents"],
      group: "Other",
      icon: <TocIcon />,
      subtext: "Auto-generated table of contents from headings",
    },
  ];
}

// Breadcrumb: shows folder path of the current note
function Breadcrumb({ noteId }: { noteId: string }) {
  const parts = noteId.split("/");
  // parts = ["folder", "subfolder", "note-file"]
  const segments = parts.slice(0, -1); // folder segments only

  return (
    <div className="flex items-center gap-0 px-4 py-1 text-xs text-text-muted/60 select-none overflow-hidden">
      <span className="shrink-0 opacity-60">Notes</span>
      {segments.map((seg, i) => {
        const path = segments.slice(0, i + 1).join("/");
        return (
          <span key={path} className="flex items-center gap-0 min-w-0">
            <ChevronRightIcon className="w-3 h-3 shrink-0 mx-0.5 opacity-40" />
            <span className="truncate hover:text-text transition-colors cursor-default">{seg}</span>
          </span>
        );
      })}
    </div>
  );
}

interface EditorProps {
  onToggleSidebar?: () => void;
  sidebarVisible?: boolean;
}

export function Editor({ onToggleSidebar, sidebarVisible }: EditorProps) {
  const {
    notes,
    currentNote,
    saveNote,
    createNote,
    selectNote,
    hasExternalChanges,
    reloadCurrentNote,
    reloadVersion,
    pinNote,
    unpinNote,
    pendingCursorLine,
    clearPendingCursorLine,
  } = useNotes();
  const { resolvedTheme } = useTheme();
  const [isSaving, setIsSaving] = useState(false);
  const [copyMenuOpen, setCopyMenuOpen] = useState(false);
  const [settings, setSettings] = useState<Settings | null>(null);
  const saveTimeoutRef = useRef<number | null>(null);
  const isLoadingRef = useRef(false);
  const scrollContainerRef = useRef<HTMLDivElement>(null);
  const currentNoteIdRef = useRef<string | null>(null);
  const needsSaveRef = useRef(false);
  const [cmdKPickerOpen, setCmdKPickerOpen] = useState(false);
  const [cmdKSelection, setCmdKSelection] = useState<{ from: number; to: number } | null>(null);
  const [storyFrontmatter, setStoryFrontmatter] = useState<StoryFrontmatter | null>(null);
  const storyFrontmatterRef = useRef<StoryFrontmatter | null>(null);
  const [notePageMeta, setNotePageMeta] = useState<NotePageMeta | null>(null);
  const notePageMetaRef = useRef<NotePageMeta | null>(null);
  const [wordCount, setWordCount] = useState({ words: 0, chars: 0 });
  const wordCountTimerRef = useRef<number | null>(null);
  const [emojiPickerOpen, setEmojiPickerOpen] = useState(false);
  const [exportMenuOpen, setExportMenuOpen] = useState(false);
  const [historyOpen, setHistoryOpen] = useState(false);
  const [versions, setVersions] = useState<{ id: string; timestamp: string; size: number }[]>([]);
  const [selectedVersion, setSelectedVersion] = useState<string | null>(null);
  const [versionContent, setVersionContent] = useState<string | null>(null);

  // Keep ref in sync with current note ID
  currentNoteIdRef.current = currentNote?.id ?? null;

  // Track notes list version for backlinks refresh (increments when notes array changes)
  const notesRef = useRef(notes);
  useEffect(() => { notesRef.current = notes; }, [notes]);
  const notesVersionRef = useRef(0);
  const [notesVersion, setNotesVersion] = useState(0);
  useEffect(() => {
    notesVersionRef.current += 1;
    setNotesVersion(notesVersionRef.current);
  }, [notes]);

  // Keep wikilink broken-link detection in sync with notes list
  useEffect(() => {
    updateNoteTitles(notes);
  }, [notes]);

  // Custom Link extension: TipTap's built-in Link.renderHTML calls isAllowedUri()
  // and strips href="" for any URL without a recognized protocol (http, https, etc.).
  // Since isAllowedUri is baked into the ProseMirror schema's toDOM closure at creation
  // time, patching options after the fact doesn't work. Instead, we override renderHTML
  // itself to render ALL hrefs as-is — no URL validation, no stripping.
  // This lets relative paths like "folder/note.md" survive into the DOM.
  const ScratchLink = TiptapLink.extend({
    inclusive: false,
    renderHTML({ HTMLAttributes }) {
      return ["a", mergeAttributes(this.options.HTMLAttributes, HTMLAttributes), 0];
    },
  }).configure({
    defaultProtocol: "https",
  });

  const editor = useCreateBlockNote({
    schema,
    // _tiptapOptions.extensions are appended after BlockNote's built-in extensions.
    // TipTap resolves by name, so the last "link" extension wins.
    _tiptapOptions: { extensions: [ScratchLink] },
  } as Parameters<typeof useCreateBlockNote>[0]);

  // Intercept link clicks BEFORE ProseMirror/TipTap processes them.
  // TipTap's Link extension uses window.open(); in desktop this can open an external browser,
  // and in web dev it navigates to localhost-relative URLs. We route note links ourselves.
  useEffect(() => {
    console.log("[LinkHandler] Attaching capture-phase click handler on document");

    const handleClick = (e: MouseEvent) => {
      const target = e.target as HTMLElement | null;
      console.log("[LinkHandler] Click event, target:", target?.tagName, target?.className?.slice(0, 50));

      const anchor = target?.closest?.("a[href]") as HTMLAnchorElement | null;
      if (!anchor) {
        console.log("[LinkHandler] No anchor found in ancestors");
        return;
      }

      const rawHref = (anchor.getAttribute("href") || "").trim();
      const resolvedHref = (anchor.href || "").trim();
      console.log("[LinkHandler] Anchor found:", { rawHref, resolvedHref });

      if (!rawHref && !resolvedHref) return;

      e.preventDefault();
      e.stopImmediatePropagation();

      const currentId = currentNoteIdRef.current;
      const allNotes = notesRef.current;
      console.log("[LinkHandler] Resolving link, currentNote:", currentId, "notes count:", allNotes.length);

      const rawResult = resolveNoteLink(rawHref, currentId, allNotes);
      if (rawResult && allNotes.some((n) => n.id === rawResult.noteId)) {
        console.log("[LinkHandler] Navigating to note (raw):", rawResult.noteId);
        setTimeout(() => selectNote(rawResult.noteId), 0);
        return;
      }

      const resolvedResult = resolveNoteLink(resolvedHref, currentId, allNotes);
      if (resolvedResult && allNotes.some((n) => n.id === resolvedResult.noteId)) {
        console.log("[LinkHandler] Navigating to note (resolved):", resolvedResult.noteId);
        setTimeout(() => selectNote(resolvedResult.noteId), 0);
        return;
      }

      if (
        rawHref.startsWith("#") ||
        resolvedHref.startsWith("#") ||
        (isLocalhostUrl(resolvedHref) && resolvedHref.includes("#"))
      ) {
        console.log("[LinkHandler] Anchor link suppressed:", rawHref || resolvedHref);
        return;
      }

      // Fallback: try selectNote directly even if note isn't in the local array
      // (the array may be incomplete; selectNote reads from disk via backend)
      const bestGuess = rawResult ?? resolvedResult;
      if (bestGuess) {
        console.log("[LinkHandler] Note not in local list, trying selectNote:", bestGuess.noteId);
        setTimeout(() => selectNote(bestGuess.noteId), 0);
        return;
      }

      console.log("[LinkHandler] Opening external link:", rawHref || resolvedHref);
      openExternalLink(rawHref, resolvedHref);
    };

    document.addEventListener("click", handleClick, true);
    return () => {
      console.log("[LinkHandler] Removing capture-phase click handler");
      document.removeEventListener("click", handleClick, true);
    };
  }, [selectNote]);

  // Fallback: handle window.open() interceptions emitted by Tauri.
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;

    listen<string>("link-navigate", (event) => {
      const href = String(event.payload || "").trim();
      if (!href) return;

      const resolved = resolveNoteLink(
        href,
        currentNoteIdRef.current,
        notesRef.current,
      );
      if (resolved && notesRef.current.some((n) => n.id === resolved.noteId)) {
        selectNote(resolved.noteId);
      } else if (resolved) {
        toast.error(`Note "${resolved.attemptedId}" not found`);
      }
    })
      .then((fn) => {
        if (cancelled) {
          fn();
        } else {
          unlisten = fn;
        }
      })
      .catch(() => {
        // Ignore listener setup failures in non-Tauri environments.
      });

    return () => {
      cancelled = true;
      if (unlisten) unlisten();
    };
  }, [selectNote]);

  // --- Link Hover Menu Logic ---
  const [hoveredLink, setHoveredLink] = useState<{
    rect: DOMRect;
    element: HTMLAnchorElement;
    href: string;
  } | null>(null);
  const hoverTimeoutRef = useRef<number | null>(null);

  useEffect(() => {
    const container = scrollContainerRef.current;
    if (!container) return;

    const handleMouseOver = (e: MouseEvent) => {
      const target = e.target as HTMLElement;
      const anchor = target.closest("a");
      const menu = target.closest(".link-hover-menu");

      if (anchor) {
        if (hoverTimeoutRef.current) {
          clearTimeout(hoverTimeoutRef.current);
          hoverTimeoutRef.current = null;
        }
        setHoveredLink({
          rect: anchor.getBoundingClientRect(),
          element: anchor,
          href: anchor.getAttribute("href") || "",
        });
      } else if (menu) {
        if (hoverTimeoutRef.current) {
          clearTimeout(hoverTimeoutRef.current);
          hoverTimeoutRef.current = null;
        }
      } else {
        if (!hoverTimeoutRef.current && hoveredLink) {
          hoverTimeoutRef.current = window.setTimeout(() => {
            setHoveredLink(null);
            hoverTimeoutRef.current = null;
          }, 300);
        }
      }
    };

    const handleScroll = () => {
      if (hoveredLink) setHoveredLink(null);
    };

    container.addEventListener("mouseover", handleMouseOver);
    container.addEventListener("scroll", handleScroll);
    return () => {
      container.removeEventListener("mouseover", handleMouseOver);
      container.removeEventListener("scroll", handleScroll);
    };
  }, [hoveredLink]);

  const handleEditLink = useCallback(() => {
    if (!hoveredLink || !editor) return;
    
    // Access internal Tiptap view to resolve DOM position
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const tiptapEditor = (editor as any)._tiptapEditor;
    if (!tiptapEditor) return;

    try {
      const pos = tiptapEditor.view.posAtDOM(hoveredLink.element, 0);
      if (pos === null || pos === undefined) return;

      const { tr } = tiptapEditor.state;
      const resolvedPos = tiptapEditor.state.doc.resolve(pos + 1);
      const selection = tiptapEditor.state.selection.constructor.near(resolvedPos);
      tr.setSelection(selection);
      tiptapEditor.view.dispatch(tr);
      tiptapEditor.view.focus();
      
      setHoveredLink(null);
    } catch (err) {
      console.error("Failed to edit link:", err);
    }
  }, [editor, hoveredLink]);

  const handleCopyLink = useCallback(async () => {
    if (hoveredLink) {
      await invoke("copy_to_clipboard", { text: hoveredLink.href });
      toast.success("Link copied");
      setHoveredLink(null);
    }
  }, [hoveredLink]);


  // Track which note's content is currently loaded in the editor
  const loadedNoteIdRef = useRef<string | null>(null);
  const loadedModifiedRef = useRef<number | null>(null);
  const lastSaveRef = useRef<{ noteId: string; content: string } | null>(null);
  const lastReloadVersionRef = useRef(0);

  // Get markdown from BlockNote editor
  const getMarkdown = useCallback(
    (editorInstance: typeof editor) => {
      try {
        return editorInstance.blocksToMarkdownLossy(editorInstance.document);
      } catch {
        return "";
      }
    },
    [],
  );

  // Load settings when note changes
  useEffect(() => {
    if (currentNote?.id) {
      notesService
        .getSettings()
        .then(setSettings)
        .catch((error) => {
          console.error("Failed to load settings:", error);
          toast.error(
            `Failed to load settings: ${
              error instanceof Error ? error.message : "Unknown error"
            }`,
          );
        });
    }
  }, [currentNote?.id]);

  const isPinned =
    settings?.pinnedNoteIds?.includes(currentNote?.id || "") || false;

  // Immediate save function
  const saveImmediately = useCallback(
    async (noteId: string, content: string) => {
      setIsSaving(true);
      try {
        // Restore SVG code blocks and callout blocks to markdown syntax
        const body = postprocessDatabaseRefs(postprocessBookmarks(postprocessEquations(postprocessCallouts(postprocessSvg(content)))));
        // Recombine frontmatter with body
        const fm = storyFrontmatterRef.current;
        const pageMeta = notePageMetaRef.current;
        let fullContent: string;
        if (fm) {
          fullContent = recombine(fm, body);
        } else if (pageMeta) {
          fullContent = recombineNoteMeta(pageMeta, body);
        } else {
          fullContent = body;
        }
        lastSaveRef.current = { noteId, content: fullContent };
        await saveNote(fullContent, noteId);
      } finally {
        setIsSaving(false);
      }
    },
    [saveNote],
  );

  // Flush any pending save immediately
  const flushPendingSave = useCallback(async () => {
    if (saveTimeoutRef.current) {
      clearTimeout(saveTimeoutRef.current);
      saveTimeoutRef.current = null;
    }

    if (needsSaveRef.current && editor && loadedNoteIdRef.current) {
      needsSaveRef.current = false;
      const markdown = getMarkdown(editor);
      await saveImmediately(loadedNoteIdRef.current, markdown);
    }
  }, [saveImmediately, getMarkdown, editor]);

  // Schedule a debounced save
  const scheduleSave = useCallback(() => {
    if (saveTimeoutRef.current) {
      clearTimeout(saveTimeoutRef.current);
    }

    const savingNoteId = currentNote?.id;
    if (!savingNoteId) return;

    needsSaveRef.current = true;

    saveTimeoutRef.current = window.setTimeout(() => {
      if (currentNoteIdRef.current !== savingNoteId || !needsSaveRef.current) {
        return;
      }

      if (editor) {
        needsSaveRef.current = false;
        const markdown = getMarkdown(editor);
        saveImmediately(savingNoteId, markdown);
      }
    }, 500);
  }, [saveImmediately, getMarkdown, currentNote?.id, editor]);

  // Handle editor content changes
  const handleEditorChange = useCallback(() => {
    if (isLoadingRef.current) return;
    scheduleSave();
    // Debounced word count update
    if (wordCountTimerRef.current) clearTimeout(wordCountTimerRef.current);
    wordCountTimerRef.current = window.setTimeout(() => {
      if (!editor) return;
      try {
        const md = editor.blocksToMarkdownLossy(editor.document);
        // Strip markdown syntax for plain text counting
        const plain = md
          .replace(/^---[\s\S]*?---\n?/m, "") // frontmatter
          .replace(/```[\s\S]*?```/g, "") // code blocks
          .replace(/`[^`]+`/g, "") // inline code
          .replace(/!\[.*?\]\(.*?\)/g, "") // images
          .replace(/\[([^\]]*)\]\(.*?\)/g, "$1") // links → text
          .replace(/[#*_~>|-]+/g, " ") // markdown symbols
          .trim();
        const words = plain ? plain.split(/\s+/).filter(Boolean).length : 0;
        const chars = plain.length;
        setWordCount({ words, chars });
      } catch {
        // Ignore count errors
      }
    }, 500);
  }, [scheduleSave, editor]);

  // Toggle page width (wide/default)
  const toggleWide = useCallback(() => {
    const current = notePageMetaRef.current || {};
    const newMeta = { ...current, wide: !current.wide };
    notePageMetaRef.current = newMeta;
    setNotePageMeta(newMeta);
    // Force save with updated frontmatter
    if (editor && loadedNoteIdRef.current) {
      const markdown = getMarkdown(editor);
      saveImmediately(loadedNoteIdRef.current, markdown);
    }
  }, [editor, getMarkdown, saveImmediately]);

  // Set page cover image
  const setPageCover = useCallback((coverPath: string | undefined) => {
    const current = notePageMetaRef.current || {};
    const newMeta = { ...current };
    if (coverPath) {
      newMeta.cover = coverPath;
      if (!newMeta.cover_position) newMeta.cover_position = 50;
    } else {
      delete newMeta.cover;
      delete newMeta.cover_position;
    }
    notePageMetaRef.current = Object.keys(newMeta).length > 0 ? newMeta : null;
    setNotePageMeta(Object.keys(newMeta).length > 0 ? newMeta : null);
    if (editor && loadedNoteIdRef.current) {
      const markdown = getMarkdown(editor);
      saveImmediately(loadedNoteIdRef.current, markdown);
    }
  }, [editor, getMarkdown, saveImmediately]);

  // Export note handlers
  const handleExportMarkdown = useCallback(async () => {
    if (!currentNote) return;
    const title = currentNote.title.replace(/[/\\:*?"<>|]/g, "_");
    const dest = await saveDialog({
      defaultPath: `${title}.md`,
      filters: [{ name: "Markdown", extensions: ["md"] }],
    });
    if (!dest) return;
    try {
      await invoke("export_note_markdown", { id: currentNote.id, dest, includeFrontmatter: false });
      toast.success("Exported as Markdown");
    } catch (e) {
      toast.error(`Export failed: ${e}`);
    }
  }, [currentNote]);

  const handleExportHtml = useCallback(async () => {
    if (!currentNote) return;
    const title = currentNote.title.replace(/[/\\:*?"<>|]/g, "_");
    const dest = await saveDialog({
      defaultPath: `${title}.html`,
      filters: [{ name: "HTML", extensions: ["html"] }],
    });
    if (!dest) return;
    try {
      await invoke("export_note_html", { id: currentNote.id, dest });
      toast.success("Exported as HTML");
    } catch (e) {
      toast.error(`Export failed: ${e}`);
    }
  }, [currentNote]);

  const handleExportPdf = useCallback(() => {
    window.print();
  }, []);

  // Version history
  const loadVersions = useCallback(async () => {
    if (!currentNote) return;
    try {
      const result = await invoke<{ id: string; timestamp: string; size: number }[]>("list_versions", { noteId: currentNote.id });
      setVersions(result);
      setSelectedVersion(null);
      setVersionContent(null);
    } catch (err) {
      console.error("Failed to load versions:", err);
      setVersions([]);
    }
  }, [currentNote]);

  const toggleHistory = useCallback(() => {
    if (!historyOpen) {
      loadVersions();
    } else {
      setSelectedVersion(null);
      setVersionContent(null);
    }
    setHistoryOpen((v) => !v);
  }, [historyOpen, loadVersions]);

  const selectVersion = useCallback(async (versionId: string) => {
    if (!currentNote) return;
    setSelectedVersion(versionId);
    try {
      const content = await invoke<string>("read_version", { noteId: currentNote.id, versionId });
      setVersionContent(content);
    } catch (err) {
      console.error("Failed to read version:", err);
      setVersionContent(null);
    }
  }, [currentNote]);

  const restoreVersion = useCallback(async (versionId: string) => {
    if (!currentNote) return;
    try {
      await invoke("restore_version", { noteId: currentNote.id, versionId });
      toast.success("Version restored");
      setHistoryOpen(false);
      setSelectedVersion(null);
      setVersionContent(null);
      // Reload the note
      selectNote(currentNote.id);
    } catch (err) {
      toast.error("Failed to restore version");
      console.error(err);
    }
  }, [currentNote, selectNote]);

  // Close history when switching notes
  useEffect(() => {
    setHistoryOpen(false);
    setSelectedVersion(null);
    setVersionContent(null);
  }, [currentNote?.id]);

  // Set page icon emoji
  const setPageIcon = useCallback((emoji: string | undefined) => {
    const current = notePageMetaRef.current || {};
    const newMeta = { ...current, icon: emoji };
    if (!emoji) delete newMeta.icon;
    notePageMetaRef.current = Object.keys(newMeta).length > 0 ? newMeta : null;
    setNotePageMeta(Object.keys(newMeta).length > 0 ? newMeta : null);
    setEmojiPickerOpen(false);
    // Force save with updated frontmatter
    if (editor && loadedNoteIdRef.current) {
      const markdown = getMarkdown(editor);
      saveImmediately(loadedNoteIdRef.current, markdown);
    }
  }, [editor, getMarkdown, saveImmediately]);

  // Extract body from content, stripping frontmatter and preprocessing SVG/callouts
  const extractBody = useCallback((content: string): string => {
    let body: string;
    const parsed = parseFrontmatter(content);
    if (parsed) {
      storyFrontmatterRef.current = parsed.frontmatter;
      setStoryFrontmatter(parsed.frontmatter);
      notePageMetaRef.current = null;
      setNotePageMeta(null);
      body = parsed.body;
    } else {
      storyFrontmatterRef.current = null;
      setStoryFrontmatter(null);
      // Check for generic note page metadata (icon, wide, cover)
      const pageMeta = parseNoteMeta(content);
      if (pageMeta) {
        notePageMetaRef.current = pageMeta.meta;
        setNotePageMeta(pageMeta.meta);
        body = pageMeta.body;
      } else {
        notePageMetaRef.current = null;
        setNotePageMeta(null);
        body = content;
      }
    }
    return preprocessSvg(preprocessDatabaseRefs(preprocessBookmarks(preprocessCallouts(body))));
  }, []);

  // Post-parse: convert paragraphs containing %%DB:name:view%% markers into databaseTable blocks
  const injectDatabaseBlocks = useCallback((blocks: any[]): any[] => {
    return blocks.map((block) => {
      if (block.type === "paragraph" && block.content) {
        const textContent = block.content
          .map((c: any) => (typeof c === "string" ? c : c.text || ""))
          .join("");
        const dbMatch = textContent.match(/%%DB:([^:]+):(\w+)%%/);
        if (dbMatch) {
          return {
            type: "databaseTable",
            props: {
              databaseName: dbMatch[1],
              view: dbMatch[2],
            },
            children: [],
          };
        }
      }
      if (block.children && block.children.length > 0) {
        return { ...block, children: injectDatabaseBlocks(block.children) };
      }
      return block;
    });
  }, []);

  // Parse markdown to blocks with fallback to raw text paragraphs
  const parseMarkdownSafe = useCallback(
    (body: string) => {
      try {
        const blocks = editor.tryParseMarkdownToBlocks(body);
        if (blocks && blocks.length > 0) {
          return injectDatabaseBlocks(injectWikilinks(blocks));
        }
      } catch (err) {
        console.error("[Editor] Parse pipeline failed:", err);
      }
      // Fallback: split markdown into paragraphs so content is at least visible
      const lines = body.split("\n");
      return lines.map((line) => ({
        type: "paragraph" as const,
        content: [{ type: "text" as const, text: line, styles: {} }],
      }));
    },
    [editor, injectDatabaseBlocks],
  );

  // Load note content when the current note changes
  useEffect(() => {
    if (!currentNote || !editor) return;

    const isSameNote = currentNote.id === loadedNoteIdRef.current;

    // Flush any pending save before switching to a different note
    if (!isSameNote && needsSaveRef.current) {
      void flushPendingSave();
    }

    const isManualReload = reloadVersion !== lastReloadVersionRef.current;

    if (isSameNote) {
      if (isManualReload) {
        lastReloadVersionRef.current = reloadVersion;
        loadedModifiedRef.current = currentNote.modified;
        isLoadingRef.current = true;

        const body = extractBody(currentNote.content);
        const blocks = parseMarkdownSafe(body);
        // Defer replaceBlocks to avoid flushSync-during-lifecycle conflict
        setTimeout(() => {
          try {
            editor.replaceBlocks(editor.document, blocks);
          } catch (err) {
            console.error("BlockNote replaceBlocks failed:", err);
          }
          isLoadingRef.current = false;
        }, 0);
        return;
      }
      loadedModifiedRef.current = currentNote.modified;
      return;
    }

    // Handle note rename
    const lastSave = lastSaveRef.current;
    if (
      lastSave?.noteId === loadedNoteIdRef.current &&
      lastSave?.content === currentNote.content
    ) {
      loadedNoteIdRef.current = currentNote.id;
      loadedModifiedRef.current = currentNote.modified;
      lastSaveRef.current = null;
      return;
    }

    const loadingNoteId = currentNote.id;

    loadedNoteIdRef.current = loadingNoteId;
    loadedModifiedRef.current = currentNote.modified;

    isLoadingRef.current = true;

    // Parse frontmatter and load body into BlockNote
    const body = extractBody(currentNote.content);
    const blocks = parseMarkdownSafe(body);
    if (loadedNoteIdRef.current !== loadingNoteId) return;

    // Defer replaceBlocks to avoid flushSync-during-lifecycle conflict.
    // BlockNote/TipTap uses flushSync internally which errors if called
    // during React's effect phase.
    setTimeout(() => {
      if (loadedNoteIdRef.current !== loadingNoteId) return;
      try {
        editor.replaceBlocks(editor.document, blocks);
      } catch (err) {
        console.error("BlockNote replaceBlocks failed:", err);
      }

      scrollContainerRef.current?.scrollTo(0, 0);

      requestAnimationFrame(() => {
        if (loadedNoteIdRef.current !== loadingNoteId) return;
        scrollContainerRef.current?.scrollTo(0, 0);
        isLoadingRef.current = false;

        // Position cursor at template cursor line if pending
        if (pendingCursorLine != null && pendingCursorLine >= 0) {
          try {
            const blocks = editor.document;
            const targetBlock = blocks[Math.min(pendingCursorLine, blocks.length - 1)];
            if (targetBlock) {
              editor.setTextCursorPosition(targetBlock.id, "end");
            }
          } catch {
            // Ignore cursor positioning errors
          }
          clearPendingCursorLine();
        }
      });
    }, 0);
  }, [currentNote, editor, flushPendingSave, reloadVersion, extractBody, parseMarkdownSafe, pendingCursorLine, clearPendingCursorLine]);

  // Scroll to top on mount
  useEffect(() => {
    scrollContainerRef.current?.scrollTo(0, 0);
  }, []);

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      if (saveTimeoutRef.current) {
        clearTimeout(saveTimeoutRef.current);
      }
      if (wordCountTimerRef.current) {
        clearTimeout(wordCountTimerRef.current);
      }
      if (needsSaveRef.current && editor) {
        needsSaveRef.current = false;
        try {
          let markdown = editor.blocksToMarkdownLossy(editor.document);
          markdown = postprocessDatabaseRefs(postprocessBookmarks(postprocessEquations(postprocessCallouts(postprocessSvg(markdown)))));
          const fm = storyFrontmatterRef.current;
          const pageMeta = notePageMetaRef.current;
          const full = fm ? recombine(fm, markdown) : pageMeta ? recombineNoteMeta(pageMeta, markdown) : markdown;
          saveNote(full);
        } catch {
          // Ignore errors during cleanup
        }
      }
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Keyboard shortcut for Cmd+Shift+C to open copy menu
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.key === "c") {
        e.preventDefault();
        setCopyMenuOpen(true);
      }
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, []);

  // Keyboard shortcut for Cmd+Shift+F to toggle page width
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.key.toLowerCase() === "f") {
        // Only handle when not in story files (which don't use notePageMeta)
        if (storyFrontmatterRef.current) return;
        e.preventDefault();
        toggleWide();
      }
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [toggleWide]);

  // Keyboard shortcut for Cmd+Shift+E to open export menu
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.key.toLowerCase() === "e") {
        if (!currentNote) return;
        e.preventDefault();
        setExportMenuOpen(true);
      }
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [currentNote]);

  // Keyboard shortcut for Cmd+Shift+H to toggle version history
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.key.toLowerCase() === "h") {
        if (!currentNote) return;
        e.preventDefault();
        toggleHistory();
      }
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [currentNote, toggleHistory]);

  // Cmd/Ctrl+K: with text selected, open note picker directly for one-step linking.
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Log all keydowns for debugging
      if (e.key.toLowerCase() === "k" && (e.metaKey || e.ctrlKey)) {
         console.log("[Cmd+K] Keydown detected", { meta: e.metaKey, ctrl: e.ctrlKey, shift: e.shiftKey, alt: e.altKey });
      }

      if (!(e.metaKey || e.ctrlKey) || e.shiftKey || e.altKey) return;
      if (e.key.toLowerCase() !== "k") return;

      let target = e.target as HTMLElement | null;
      console.log("[Cmd+K] Initial target:", target);

      // Handle text nodes (nodeType 3)
      if (target && target.nodeType === 3) {
        target = target.parentElement;
      }
      
      const closestEditor = target?.closest?.(".bn-editor");
      console.log("[Cmd+K] Closest editor:", closestEditor);

      if (!closestEditor) return;

      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const tiptap = (editor as any)?._tiptapEditor;
      const selection = tiptap?.state?.selection;
      
      console.log("[Cmd+K] Tiptap state:", { 
        hasTiptap: !!tiptap, 
        selectionEmpty: selection?.empty, 
        selectionFrom: selection?.from, 
        selectionTo: selection?.to 
      });

      if (!selection || selection.empty) {
        console.log("[Cmd+K] Aborting: No selection or empty selection");
        return;
      }

      console.log("[Cmd+K] activating picker...");
      e.preventDefault();
      e.stopPropagation();
      e.stopImmediatePropagation(); // Ensure no other handlers see this
      
      setCmdKSelection({ from: selection.from, to: selection.to });
      setCmdKPickerOpen(true);
    };

    console.log("[Editor] Registering Cmd+K handler");
    document.addEventListener("keydown", handleKeyDown, true);
    return () => {
      console.log("[Editor] Unregistering Cmd+K handler");
      document.removeEventListener("keydown", handleKeyDown, true);
    };
  }, [editor]);

  const handleCmdKNotePicked = useCallback(
    (noteId: string) => {
      console.log("[handleCmdKNotePicked] Picked:", noteId, { cmdKSelection });
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const tiptap = (editor as any)?._tiptapEditor;
      if (!tiptap || !cmdKSelection) {
        console.warn("[handleCmdKNotePicked] Missing tiptap instance or selection");
        return;
      }

      const relativePath = currentNoteIdRef.current
        ? computeRelativePath(currentNoteIdRef.current, noteId)
        : noteId.endsWith(".md")
          ? noteId
          : `${noteId}.md`;
      
      console.log("[handleCmdKNotePicked] Applying link:", relativePath);

      tiptap
        .chain()
        .focus()
        .setTextSelection(cmdKSelection)
        .setLink({ href: relativePath })
        .run();

      setCmdKPickerOpen(false);
      setCmdKSelection(null);
    },
    [editor, cmdKSelection],
  );

  // Wikilink navigation: listen for click events from wikilink nodes
  useEffect(() => {
    const handleNavigate = (e: Event) => {
      const title = (e as CustomEvent).detail?.title;
      if (!title) return;
      const target = notes.find(
        (n) => n.title.toLowerCase() === title.toLowerCase(),
      );
      if (target) {
        selectNote(target.id);
      } else {
        toast.error(`Note "${title}" not found`);
      }
    };
    window.addEventListener("wikilink-navigate", handleNavigate);
    return () => window.removeEventListener("wikilink-navigate", handleNavigate);
  }, [notes, selectNote]);



  // Wikilink suggestion menu items
  const getWikilinkItems = useCallback(
    async (query: string) => {
      return getWikilinkMenuItems(editor, notes, query);
    },
    [editor, notes],
  );

  // Image handler
  const handleAddImage = useCallback(async () => {
    if (!editor) return;
    const selected = await openDialog({
      multiple: false,
      filters: [
        {
          name: "Images",
          extensions: ["png", "jpg", "jpeg", "gif", "webp", "svg"],
        },
      ],
    });
    if (selected) {
      try {
        const relativePath = await invoke<string>("copy_image_to_assets", {
          sourcePath: selected as string,
        });
        const notesFolder = await invoke<string>("get_notes_folder");
        const absolutePath = await join(notesFolder, relativePath);
        const assetUrl = convertFileSrc(absolutePath);

        const cursorBlock = editor.getTextCursorPosition().block;
        editor.insertBlocks(
          [
            {
              type: "image",
              props: { url: assetUrl },
            },
          ],
          cursorBlock,
          "after",
        );
      } catch (error) {
        console.error("Failed to add image:", error);
        toast.error("Failed to add image");
      }
    }
  }, [editor]);

  // Handle frontmatter changes from StoryMetaCard (e.g. status dropdown)
  const handleFrontmatterChange = useCallback(
    (updated: StoryFrontmatter) => {
      storyFrontmatterRef.current = updated;
      setStoryFrontmatter(updated);
      scheduleSave();
    },
    [scheduleSave],
  );

  // Copy handlers
  const handleCopyMarkdown = useCallback(async () => {
    if (!editor) return;
    try {
      const markdown = getMarkdown(editor);
      await invoke("copy_to_clipboard", { text: markdown });
      toast.success("Copied as Markdown");
    } catch (error) {
      console.error("Failed to copy markdown:", error);
      toast.error("Failed to copy");
    }
  }, [editor, getMarkdown]);

  const handleCopyPlainText = useCallback(async () => {
    if (!editor) return;
    try {
      // Convert to markdown then strip formatting for plain text
      const markdown = getMarkdown(editor);
      const text = markdown
        .replace(/^#{1,6}\s+/gm, "") // strip heading markers
        .replace(/\*\*(.+?)\*\*/g, "$1") // bold
        .replace(/\*(.+?)\*/g, "$1") // italic
        .replace(/`(.+?)`/g, "$1") // inline code
        .replace(/\[(.+?)\]\(.+?\)/g, "$1") // links
        .replace(/!\[.*?\]\(.+?\)/g, "") // images
        .replace(/^[-*+]\s+/gm, "- ") // normalize list markers
        .replace(/^\d+\.\s+/gm, (m) => m); // keep numbered lists
      await invoke("copy_to_clipboard", { text });
      toast.success("Copied as plain text");
    } catch (error) {
      console.error("Failed to copy plain text:", error);
      toast.error("Failed to copy");
    }
  }, [editor, getMarkdown]);

  const handleCopyHtml = useCallback(async () => {
    if (!editor) return;
    try {
      const html = editor.blocksToHTMLLossy(editor.document);
      await invoke("copy_to_clipboard", { text: html });
      toast.success("Copied as HTML");
    } catch (error) {
      console.error("Failed to copy HTML:", error);
      toast.error("Failed to copy");
    }
  }, [editor]);

  if (!currentNote) {
    return (
      <div className="flex-1 flex flex-col bg-bg">
        <div
          className="h-10 shrink-0 flex items-end px-4 pb-1"
          data-tauri-drag-region
        ></div>
        <div className="flex-1 flex items-center justify-center pb-8">
          <div className="text-center text-text-muted select-none">
            <img
              src="/note-dark.png"
              alt="Note"
              className="w-42 h-auto mx-auto mb-1 invert dark:invert-0"
            />
            <h1 className="text-2xl text-text font-serif mb-1 tracking-[-0.01em]">
              What's on your mind?
            </h1>
            <p className="text-sm">
              Pick up where you left off, or start something new
            </p>
            <Button
              onClick={createNote}
              variant="secondary"
              size="md"
              className="mt-4"
            >
              New Note{" "}
              <span className="text-text-muted ml-1">
                {mod}
                {isMac ? "" : "+"}N
              </span>
            </Button>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="flex-1 flex flex-col bg-bg overflow-hidden">
      {/* Drag region with sidebar toggle, date and save status */}
      <div
        className={cn(
          "h-11 shrink-0 flex items-center justify-between px-3",
          !sidebarVisible && "pl-22",
        )}
        data-tauri-drag-region
      >
        <div className="titlebar-no-drag flex items-center gap-1 min-w-0">
          {onToggleSidebar && (
            <IconButton
              onClick={onToggleSidebar}
              title={
                sidebarVisible
                  ? `Hide sidebar (${mod}${isMac ? "" : "+"}\\)`
                  : `Show sidebar (${mod}${isMac ? "" : "+"}\\)`
              }
              className="shrink-0"
            >
              <PanelLeftIcon className="w-4.5 h-4.5 stroke-[1.5]" />
            </IconButton>
          )}
          <span className="text-xs text-text-muted mb-px truncate">
            {formatDateTime(currentNote.modified)}
          </span>
        </div>
        <div className="titlebar-no-drag flex items-center gap-px shrink-0">
          {hasExternalChanges ? (
            <Tooltip
              content={`External changes detected (${mod}${isMac ? "" : "+"}R to refresh)`}
            >
              <button
                onClick={reloadCurrentNote}
                className="h-7 px-2 flex items-center gap-1 text-xs text-orange-500 hover:bg-orange-500/10 rounded transition-colors font-medium"
              >
                <RefreshCwIcon className="w-4 h-4 stroke-[1.6]" />
                <span>Refresh</span>
              </button>
            </Tooltip>
          ) : isSaving ? (
            <Tooltip content="Saving...">
              <div className="h-7 w-7 flex items-center justify-center">
                <SpinnerIcon className="w-4.5 h-4.5 text-text-muted/40 stroke-[1.5] animate-spin" />
              </div>
            </Tooltip>
          ) : (
            <Tooltip content="All changes saved">
              <div className="h-7 w-7 flex items-center justify-center rounded-full">
                <CircleCheckIcon className="w-4.5 h-4.5 mt-px stroke-[1.5] text-text-muted/40" />
              </div>
            </Tooltip>
          )}
          <Tooltip content={isPinned ? "Unpin note" : "Pin note"}>
            <IconButton
              onClick={async () => {
                try {
                  if (isPinned) {
                    await unpinNote(currentNote.id);
                    toast.success("Note unpinned");
                  } else {
                    await pinNote(currentNote.id);
                    toast.success("Note pinned");
                  }
                  const updatedSettings = await notesService.getSettings();
                  setSettings(updatedSettings);
                } catch (error) {
                  console.error("Failed to pin/unpin note:", error);
                  toast.error(
                    `Failed to ${isPinned ? "unpin" : "pin"} note: ${
                      error instanceof Error ? error.message : "Unknown error"
                    }`,
                  );
                }
              }}
            >
              <PinIcon
                className={cn(
                  "w-5 h-5 stroke-[1.3]",
                  isPinned && "fill-current",
                )}
              />
            </IconButton>
          </Tooltip>
          <Tooltip content="Add image">
            <IconButton onClick={handleAddImage}>
              <ImageIcon className="w-4.25 h-4.25 stroke-[1.6]" />
            </IconButton>
          </Tooltip>
          <DropdownMenu.Root open={copyMenuOpen} onOpenChange={setCopyMenuOpen}>
            <Tooltip
              content={`Copy as... (${mod}${isMac ? "" : "+"}${shift}${isMac ? "" : "+"}C)`}
            >
              <DropdownMenu.Trigger asChild>
                <IconButton>
                  <CopyIcon className="w-4.25 h-4.25 stroke-[1.6]" />
                </IconButton>
              </DropdownMenu.Trigger>
            </Tooltip>
            <DropdownMenu.Portal>
              <DropdownMenu.Content
                className="min-w-35 bg-bg border border-border rounded-md shadow-lg py-1 z-50"
                sideOffset={5}
                align="end"
                onCloseAutoFocus={(e) => e.preventDefault()}
                onKeyDown={(e) => {
                  if (e.key === "ArrowUp" || e.key === "ArrowDown") {
                    e.stopPropagation();
                  }
                }}
              >
                <DropdownMenu.Item
                  className="px-3 py-1.5 text-sm text-text cursor-pointer outline-none hover:bg-bg-muted focus:bg-bg-muted"
                  onSelect={handleCopyMarkdown}
                >
                  Markdown
                </DropdownMenu.Item>
                <DropdownMenu.Item
                  className="px-3 py-1.5 text-sm text-text cursor-pointer outline-none hover:bg-bg-muted focus:bg-bg-muted"
                  onSelect={handleCopyPlainText}
                >
                  Plain Text
                </DropdownMenu.Item>
                <DropdownMenu.Item
                  className="px-3 py-1.5 text-sm text-text cursor-pointer outline-none hover:bg-bg-muted focus:bg-bg-muted"
                  onSelect={handleCopyHtml}
                >
                  HTML
                </DropdownMenu.Item>
              </DropdownMenu.Content>
            </DropdownMenu.Portal>
          </DropdownMenu.Root>
          <DropdownMenu.Root open={exportMenuOpen} onOpenChange={setExportMenuOpen}>
            <Tooltip
              content={`Export... (${mod}${isMac ? "" : "+"}${shift}${isMac ? "" : "+"}E)`}
            >
              <DropdownMenu.Trigger asChild>
                <IconButton>
                  <ArrowDownToLineIcon className="w-4.25 h-4.25 stroke-[1.6]" />
                </IconButton>
              </DropdownMenu.Trigger>
            </Tooltip>
            <DropdownMenu.Portal>
              <DropdownMenu.Content
                className="min-w-35 bg-bg border border-border rounded-md shadow-lg py-1 z-50"
                sideOffset={5}
                align="end"
                onCloseAutoFocus={(e) => e.preventDefault()}
                onKeyDown={(e) => {
                  if (e.key === "ArrowUp" || e.key === "ArrowDown") {
                    e.stopPropagation();
                  }
                }}
              >
                <DropdownMenu.Item
                  className="px-3 py-1.5 text-sm text-text cursor-pointer outline-none hover:bg-bg-muted focus:bg-bg-muted"
                  onSelect={handleExportMarkdown}
                >
                  Markdown
                </DropdownMenu.Item>
                <DropdownMenu.Item
                  className="px-3 py-1.5 text-sm text-text cursor-pointer outline-none hover:bg-bg-muted focus:bg-bg-muted"
                  onSelect={handleExportHtml}
                >
                  HTML
                </DropdownMenu.Item>
                <DropdownMenu.Item
                  className="px-3 py-1.5 text-sm text-text cursor-pointer outline-none hover:bg-bg-muted focus:bg-bg-muted"
                  onSelect={handleExportPdf}
                >
                  PDF (Print)
                </DropdownMenu.Item>
              </DropdownMenu.Content>
            </DropdownMenu.Portal>
          </DropdownMenu.Root>

          {/* Version History */}
          <Tooltip content={`Version history (${mod}${isMac ? "" : "+"}${shift}${isMac ? "" : "+"}H)`}>
            <IconButton
              onClick={toggleHistory}
              className={cn("h-7 w-7", historyOpen && "bg-bg-muted")}
            >
              <ClockIcon className="w-4 h-4" />
            </IconButton>
          </Tooltip>
        </div>
      </div>

      {/* Breadcrumb */}
      {currentNote.id.includes("/") && (
        <Breadcrumb noteId={currentNote.id} />
      )}

      {/* BlockNote Editor */}
      <div
        ref={scrollContainerRef}
        className="flex-1 overflow-y-auto"
      >
        {/* Page cover image */}
        {!storyFrontmatter && notePageMeta?.cover && (
          <div
            className="page-cover group relative w-full h-48 overflow-hidden cursor-pointer"
            style={{
              backgroundImage: `url(${convertFileSrc(notePageMeta.cover)})`,
              backgroundSize: "cover",
              backgroundPosition: `center ${notePageMeta.cover_position ?? 50}%`,
            }}
            onMouseDown={(e) => {
              // Drag to reposition
              const startY = e.clientY;
              const startPos = notePageMeta.cover_position ?? 50;
              const handler = (ev: MouseEvent) => {
                const delta = ((ev.clientY - startY) / 192) * 100; // 192 = cover height
                const newPos = Math.max(0, Math.min(100, startPos + delta));
                const current = notePageMetaRef.current || {};
                notePageMetaRef.current = { ...current, cover_position: Math.round(newPos) };
                setNotePageMeta({ ...current, cover_position: Math.round(newPos) });
              };
              const up = () => {
                document.removeEventListener("mousemove", handler);
                document.removeEventListener("mouseup", up);
                // Save the new position
                if (editor && loadedNoteIdRef.current) {
                  const md = getMarkdown(editor);
                  saveImmediately(loadedNoteIdRef.current, md);
                }
              };
              document.addEventListener("mousemove", handler);
              document.addEventListener("mouseup", up);
            }}
          >
            <div className="absolute inset-x-0 bottom-0 flex justify-end gap-1 p-2 opacity-0 group-hover:opacity-100 transition-opacity">
              <button
                className="px-2 py-1 text-xs bg-black/50 text-white rounded hover:bg-black/70 transition-colors"
                onClick={async (e) => {
                  e.stopPropagation();
                  const path = await openDialog({
                    multiple: false,
                    filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "webp", "gif"] }],
                  });
                  if (path) {
                    try {
                      const assetPath = await invoke<string>("copy_image_to_assets", { sourcePath: path });
                      setPageCover(assetPath);
                    } catch (err) {
                      toast.error(`Failed to copy image: ${err}`);
                    }
                  }
                }}
              >
                Change cover
              </button>
              <button
                className="px-2 py-1 text-xs bg-black/50 text-white rounded hover:bg-black/70 transition-colors"
                onClick={(e) => {
                  e.stopPropagation();
                  setPageCover(undefined);
                }}
              >
                Remove
              </button>
            </div>
          </div>
        )}
        <div className={cn(
          "mx-auto px-12 pt-4 pb-24 transition-[max-width] duration-300",
          notePageMeta?.wide ? "max-w-full" : "max-w-3xl",
        )}>
          {storyFrontmatter && (
            <StoryMetaCard
              frontmatter={storyFrontmatter}
              onChange={handleFrontmatterChange}
            />
          )}
          {/* Page icon + cover controls for regular notes */}
          {!storyFrontmatter && (
            <div className="page-icon-area group relative mb-1 flex items-center gap-2">
              {notePageMeta?.icon ? (
                <div className="relative inline-block">
                  <button
                    className="page-icon-display"
                    onClick={() => setEmojiPickerOpen(true)}
                    onContextMenu={(e) => {
                      e.preventDefault();
                      setPageIcon(undefined);
                    }}
                    title="Click to change, right-click to remove"
                  >
                    {notePageMeta.icon}
                  </button>
                  {emojiPickerOpen && (
                    <div className="absolute top-full left-0 mt-1 z-50">
                      <EmojiPicker
                        onSelect={(emoji) => setPageIcon(emoji)}
                        onClose={() => setEmojiPickerOpen(false)}
                      />
                    </div>
                  )}
                </div>
              ) : (
                <div className="relative inline-block">
                  <button
                    className="page-icon-add opacity-0 group-hover:opacity-100 transition-opacity"
                    onClick={() => {
                      setPageIcon(getRandomEmoji());
                      setEmojiPickerOpen(true);
                    }}
                  >
                    Add icon
                  </button>
                  {emojiPickerOpen && (
                    <div className="absolute top-full left-0 mt-1 z-50">
                      <EmojiPicker
                        onSelect={(emoji) => setPageIcon(emoji)}
                        onClose={() => setEmojiPickerOpen(false)}
                      />
                    </div>
                  )}
                </div>
              )}
              {!notePageMeta?.cover && (
                <button
                  className="page-icon-add opacity-0 group-hover:opacity-100 transition-opacity"
                  onClick={async () => {
                    const path = await openDialog({
                      multiple: false,
                      filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "webp", "gif"] }],
                    });
                    if (path) {
                      try {
                        const assetPath = await invoke<string>("copy_image_to_assets", { sourcePath: path });
                        setPageCover(assetPath);
                      } catch (err) {
                        toast.error(`Failed to copy image: ${err}`);
                      }
                    }
                  }}
                >
                  Add cover
                </button>
              )}
            </div>
          )}
          <BlockNoteView
            editor={editor}
            theme={resolvedTheme}
            onChange={handleEditorChange}
            slashMenu={false}
            linkToolbar={false}
            formattingToolbar={false}
          >
            <FormattingToolbarController
              formattingToolbar={() => (
                <FormattingToolbar>
                  <BlockTypeSelect key="blockTypeSelect" />
                  <BasicTextStyleButton basicTextStyle="bold" key="boldStyleButton" />
                  <BasicTextStyleButton basicTextStyle="italic" key="italicStyleButton" />
                  <BasicTextStyleButton basicTextStyle="underline" key="underlineStyleButton" />
                  <BasicTextStyleButton basicTextStyle="strike" key="strikeStyleButton" />
                  <TextAlignButton textAlignment="center" key="textAlignCenterButton" />
                  <TextAlignButton textAlignment="left" key="textAlignLeftButton" />
                  <TextAlignButton textAlignment="right" key="textAlignRightButton" />
                  <ColorStyleButton key="colorStyleButton" />
                  <NestBlockButton key="nestBlockButton" />
                  <UnnestBlockButton key="unnestBlockButton" />
                  <CustomCreateLinkButton
                    key="createLinkButton"
                    notes={notesRef.current}
                    currentNoteIdRef={currentNoteIdRef}
                    editorInstance={editor}
                  />
                  <CustomUrlLinkButton
                    key="urlLinkButton"
                    editorInstance={editor}
                  />
                </FormattingToolbar>
              )}
            />
            <LinkToolbarController
              linkToolbar={(props) => (
                <CustomLinkToolbar
                  {...props}
                  notes={notesRef.current}
                  selectNote={selectNote}
                  currentNoteId={currentNoteIdRef.current}
                  editor={editor}
                />
              )}
            />
            <SuggestionMenuController
              triggerCharacter="/"
              getItems={async (query) =>
                filterSuggestionItems(
                  getCustomSlashMenuItems(editor),
                  query,
                )
              }
            />
            <SuggestionMenuController
              triggerCharacter="[["
              getItems={getWikilinkItems}
            />
          </BlockNoteView>
          <BacklinksPanel
            noteTitle={currentNote.title}
            noteId={currentNote.id}
            refreshTrigger={notesVersion}
            onNavigate={(noteId) => selectNote(noteId)}
          />
        </div>

        {/* Version History Panel */}
        {historyOpen && (
          <div className="w-72 shrink-0 border-l border-border bg-bg-secondary flex flex-col overflow-hidden">
            <div className="flex items-center justify-between px-3 py-2 border-b border-border">
              <span className="text-xs font-medium text-text">Version History</span>
              <IconButton onClick={() => setHistoryOpen(false)} className="h-5 w-5">
                <XIcon className="w-3 h-3" />
              </IconButton>
            </div>
            {versions.length === 0 ? (
              <div className="flex-1 flex items-center justify-center text-xs text-text-muted p-4 text-center">
                No versions yet. Versions are saved automatically as you edit.
              </div>
            ) : (
              <div className="flex-1 overflow-y-auto">
                {versions.map((v) => {
                  const date = new Date(v.timestamp);
                  const isSelected = selectedVersion === v.id;
                  return (
                    <div
                      key={v.id}
                      className={cn(
                        "px-3 py-2 border-b border-border cursor-pointer transition-colors",
                        isSelected ? "bg-bg-muted" : "hover:bg-bg-muted/50"
                      )}
                      onClick={() => selectVersion(v.id)}
                    >
                      <div className="text-xs font-medium text-text">
                        {date.toLocaleDateString(undefined, { month: "short", day: "numeric", year: "numeric" })}
                        {" "}
                        {date.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" })}
                      </div>
                      <div className="text-2xs text-text-muted mt-0.5">
                        {v.size < 1024 ? `${v.size} B` : `${(v.size / 1024).toFixed(1)} KB`}
                      </div>
                      {isSelected && (
                        <button
                          className="mt-1.5 flex items-center gap-1 text-2xs text-accent hover:underline"
                          onClick={(e) => {
                            e.stopPropagation();
                            restoreVersion(v.id);
                          }}
                        >
                          <RotateCcwIcon className="w-3 h-3" />
                          Restore this version
                        </button>
                      )}
                    </div>
                  );
                })}
              </div>
            )}
            {selectedVersion && versionContent !== null && (
              <div className="border-t border-border max-h-48 overflow-y-auto">
                <div className="px-3 py-1.5 text-2xs font-medium text-text-muted bg-bg">Preview</div>
                <pre className="px-3 py-2 text-2xs text-text whitespace-pre-wrap font-mono leading-relaxed">
                  {versionContent.slice(0, 2000)}
                  {versionContent.length > 2000 && "…"}
                </pre>
              </div>
            )}
          </div>
        )}
      </div>

      {/* Word count footer */}
      <div className="h-6 shrink-0 flex items-center justify-end px-4 border-t border-border text-2xs text-text-muted gap-3 select-none">
        <span>{wordCount.words} words</span>
        <span>{wordCount.chars} characters</span>
        <span>{Math.max(1, Math.ceil(wordCount.words / 200))} min read</span>
        {!storyFrontmatter && (
          <Tooltip content={`Toggle wide layout (${mod}${isMac ? "" : "+"}${shift}${isMac ? "" : "+"}F)`}>
            <button
              onClick={toggleWide}
              className={cn(
                "px-1.5 py-0.5 rounded text-2xs transition-colors",
                notePageMeta?.wide
                  ? "bg-accent text-text-inverse"
                  : "hover:bg-bg-muted"
              )}
            >
              {notePageMeta?.wide ? "Wide" : "Narrow"}
            </button>
          </Tooltip>
        )}
      </div>

      {/* Link Hover Menu */}
      {hoveredLink && (
        <div
          className="link-hover-menu fixed z-50 flex items-center gap-1 p-1 bg-bg-secondary border border-border rounded-md shadow-lg animate-fade-in"
          style={{
            top: `${hoveredLink.rect.bottom + 5}px`,
            left: `${hoveredLink.rect.left}px`,
          }}
          onMouseEnter={() => {
            if (hoverTimeoutRef.current) {
              clearTimeout(hoverTimeoutRef.current);
              hoverTimeoutRef.current = null;
            }
          }}
          onMouseLeave={() => {
            if (!hoverTimeoutRef.current) {
              hoverTimeoutRef.current = window.setTimeout(() => {
                setHoveredLink(null);
                hoverTimeoutRef.current = null;
              }, 300);
            }
          }}
        >
          <Tooltip content="Edit link">
            <IconButton onClick={handleEditLink} className="h-6 w-6">
              <EditIcon className="w-3.5 h-3.5" />
            </IconButton>
          </Tooltip>
          <div className="w-px h-3 bg-border" />
          <Tooltip content="Copy URL">
            <IconButton onClick={handleCopyLink} className="h-6 w-6">
              <CopyIcon className="w-3.5 h-3.5" />
            </IconButton>
          </Tooltip>
        </div>
      )}
      {cmdKPickerOpen && (
        <NotePickerPopover
          notes={notes}
          anchorRect={null}
          onSelect={(noteId) => {
            console.log("[Editor] Picker selected:", noteId);
            handleCmdKNotePicked(noteId);
          }}
          onClose={() => {
            console.log("[Editor] Picker closed");
            setCmdKPickerOpen(false);
            setCmdKSelection(null);
          }}
        />
      )}
    </div>
  );
}
