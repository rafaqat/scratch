import { useEffect, useRef, useCallback, useState } from "react";
import {
  useCreateBlockNote,
  SuggestionMenuController,
  getDefaultReactSlashMenuItems,
} from "@blocknote/react";
import {
  filterSuggestionItems,
  insertOrUpdateBlockForSlashMenu,
} from "@blocknote/core/extensions";
import { BlockNoteView } from "@blocknote/mantine";
import "@blocknote/core/fonts/inter.css";
import "@blocknote/mantine/style.css";
import "katex/dist/katex.min.css";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { join } from "@tauri-apps/api/path";
import { toast } from "sonner";
import { mod, shift, isMac } from "../../lib/platform";
import { parseFrontmatter, recombine, type StoryFrontmatter } from "../../lib/frontmatter";
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
} from "../icons";

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

/**
 * Build custom slash menu items with the default items plus TOC, Callout, Equation, and Bookmark.
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
  const [storyFrontmatter, setStoryFrontmatter] = useState<StoryFrontmatter | null>(null);
  const storyFrontmatterRef = useRef<StoryFrontmatter | null>(null);

  // Keep ref in sync with current note ID
  currentNoteIdRef.current = currentNote?.id ?? null;

  // Track notes list version for backlinks refresh (increments when notes array changes)
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

  // Create BlockNote editor with custom schema (wikilinks)
  const editor = useCreateBlockNote({ schema });

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
        // Recombine frontmatter with body if this is a story file
        const fm = storyFrontmatterRef.current;
        const fullContent = fm ? recombine(fm, body) : body;
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
  }, [scheduleSave]);

  // Extract body from content, stripping frontmatter and preprocessing SVG/callouts
  const extractBody = useCallback((content: string): string => {
    let body: string;
    const parsed = parseFrontmatter(content);
    if (parsed) {
      storyFrontmatterRef.current = parsed.frontmatter;
      setStoryFrontmatter(parsed.frontmatter);
      body = parsed.body;
    } else {
      storyFrontmatterRef.current = null;
      setStoryFrontmatter(null);
      body = content;
    }
    return preprocessSvg(preprocessDatabaseRefs(preprocessBookmarks(preprocessCallouts(body))));
  }, []);

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

        try {
          const body = extractBody(currentNote.content);
          const blocks = injectWikilinks(editor.tryParseMarkdownToBlocks(body));
          editor.replaceBlocks(editor.document, blocks);
        } catch {
          // Fallback: ignore parse errors
        }
        isLoadingRef.current = false;
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
    try {
      const body = extractBody(currentNote.content);
      const blocks = injectWikilinks(editor.tryParseMarkdownToBlocks(body));
      if (loadedNoteIdRef.current !== loadingNoteId) return;
      editor.replaceBlocks(editor.document, blocks);
    } catch {
      // Fallback: ignore parse errors
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
  }, [currentNote, editor, flushPendingSave, reloadVersion, extractBody, pendingCursorLine, clearPendingCursorLine]);

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
      if (needsSaveRef.current && editor) {
        needsSaveRef.current = false;
        try {
          let markdown = editor.blocksToMarkdownLossy(editor.document);
          markdown = postprocessDatabaseRefs(postprocessBookmarks(postprocessEquations(postprocessCallouts(postprocessSvg(markdown)))));
          const fm = storyFrontmatterRef.current;
          saveNote(fm ? recombine(fm, markdown) : markdown);
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
        </div>
      </div>

      {/* BlockNote Editor */}
      <div
        ref={scrollContainerRef}
        className="flex-1 overflow-y-auto"
      >
        <div className="max-w-3xl mx-auto px-12 pt-4 pb-24">
          {storyFrontmatter && (
            <StoryMetaCard
              frontmatter={storyFrontmatter}
              onChange={handleFrontmatterChange}
            />
          )}
          <BlockNoteView
            editor={editor}
            theme={resolvedTheme}
            onChange={handleEditorChange}
            slashMenu={false}
          >
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
      </div>
    </div>
  );
}
