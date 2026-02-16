import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import * as DropdownMenu from "@radix-ui/react-dropdown-menu";
import { toast } from "sonner";
import { useNotes } from "../../context/NotesContext";
import { useTheme } from "../../context/ThemeContext";
import { NoteList } from "../notes/NoteList";
import { Footer } from "./Footer";
import { IconButton, Input, Tooltip, Button } from "../ui";
import * as templatesService from "../../services/templates";
import type { TemplateInfo } from "../../services/templates";
import {
  PlusIcon,
  XIcon,
  SpinnerIcon,
  SearchIcon,
  SearchOffIcon,
  FolderIcon,
  TemplateIcon,
  AddNoteIcon,
  TrashIcon,
} from "../icons";
import { mod, isMac } from "../../lib/platform";

interface TrashedNote {
  id: string;
  title: string;
  originalPath: string;
  deletedAt: string;
  preview: string;
}

function formatTrashDate(isoString: string): string {
  const date = new Date(isoString);
  const now = new Date();
  const diffMs = now.getTime() - date.getTime();
  const diffDays = Math.floor(diffMs / 86400000);

  if (diffDays === 0) return "Deleted today";
  if (diffDays === 1) return "Deleted yesterday";
  if (diffDays < 7) return `Deleted ${diffDays} days ago`;
  return `Deleted ${date.toLocaleDateString([], { month: "short", day: "numeric" })}`;
}

interface SidebarProps {
  onOpenSettings?: () => void;
}

export function Sidebar({ onOpenSettings }: SidebarProps) {
  const { createNote, createNoteFromTemplate, notes, notesFolder, setNotesFolder, search, searchQuery, clearSearch, isSearching } =
    useNotes();
  const { reloadSettings } = useTheme();
  const [searchOpen, setSearchOpen] = useState(false);
  const [inputValue, setInputValue] = useState(searchQuery);
  const [templates, setTemplates] = useState<TemplateInfo[]>([]);
  const [trashOpen, setTrashOpen] = useState(false);
  const [trashItems, setTrashItems] = useState<TrashedNote[]>([]);
  const [trashCount, setTrashCount] = useState(0);
  const debounceRef = useRef<number | null>(null);
  const searchInputRef = useRef<HTMLInputElement>(null);

  // Load templates when folder is set
  useEffect(() => {
    if (notesFolder) {
      templatesService.listTemplates().then(setTemplates).catch(console.error);
    }
  }, [notesFolder]);

  // Sync input with search query
  useEffect(() => {
    setInputValue(searchQuery);
  }, [searchQuery]);

  const handleSearchChange = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      const value = e.target.value;
      setInputValue(value);

      // Debounce search
      if (debounceRef.current) {
        clearTimeout(debounceRef.current);
      }

      debounceRef.current = window.setTimeout(() => {
        search(value);
      }, 150);
    },
    [search]
  );

  const toggleSearch = useCallback(() => {
    setSearchOpen((prev) => !prev);
  }, []);

  const closeSearch = useCallback(() => {
    setSearchOpen(false);
    setInputValue("");
    clearSearch();
  }, [clearSearch]);

  // Auto-focus search input when opened
  useEffect(() => {
    if (searchOpen) {
      // Small delay to ensure the input is rendered
      requestAnimationFrame(() => {
        searchInputRef.current?.focus();
      });
    }
  }, [searchOpen]);

  const handleSearchKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLInputElement>) => {
      if (e.key === "Escape") {
        e.preventDefault();
        if (inputValue) {
          // First escape: clear search
          setInputValue("");
          clearSearch();
        } else {
          // Second escape: close search
          closeSearch();
        }
      }
    },
    [inputValue, clearSearch, closeSearch]
  );

  const handleClearSearch = useCallback(() => {
    setInputValue("");
    clearSearch();
  }, [clearSearch]);

  const handleChangeFolder = useCallback(async () => {
    try {
      const selected = await invoke<string | null>("open_folder_dialog", {
        defaultPath: notesFolder || null,
      });
      if (selected) {
        await setNotesFolder(selected);
        await reloadSettings();
      }
    } catch (err) {
      console.error("Failed to select folder:", err);
    }
  }, [notesFolder, setNotesFolder, reloadSettings]);

  // Load trash count on mount and when notes change
  const loadTrashCount = useCallback(async () => {
    try {
      const items = await invoke<TrashedNote[]>("list_trash");
      setTrashCount(items.length);
      if (trashOpen) setTrashItems(items);
    } catch {
      // Ignore - trash may not exist yet
    }
  }, [trashOpen]);

  useEffect(() => {
    if (notesFolder) loadTrashCount();
  }, [notesFolder, notes, loadTrashCount]);

  const toggleTrash = useCallback(async () => {
    if (!trashOpen) {
      try {
        const items = await invoke<TrashedNote[]>("list_trash");
        setTrashItems(items);
        setTrashCount(items.length);
      } catch {
        setTrashItems([]);
      }
    }
    setTrashOpen((prev) => !prev);
  }, [trashOpen]);

  const handleRestore = useCallback(async (id: string) => {
    try {
      await invoke("restore_note", { id });
      toast.success("Note restored");
      const items = await invoke<TrashedNote[]>("list_trash");
      setTrashItems(items);
      setTrashCount(items.length);
    } catch (e) {
      toast.error(`Failed to restore: ${e}`);
    }
  }, []);

  const handleDeletePermanently = useCallback(async (id: string) => {
    try {
      await invoke("delete_permanently", { id });
      const items = await invoke<TrashedNote[]>("list_trash");
      setTrashItems(items);
      setTrashCount(items.length);
    } catch (e) {
      toast.error(`Failed to delete: ${e}`);
    }
  }, []);

  const handleEmptyTrash = useCallback(async () => {
    try {
      const count = await invoke<number>("empty_trash");
      toast.success(`Permanently deleted ${count} note${count === 1 ? "" : "s"}`);
      setTrashItems([]);
      setTrashCount(0);
    } catch (e) {
      toast.error(`Failed to empty trash: ${e}`);
    }
  }, []);

  // Extract just the folder name from the full path
  const folderName = notesFolder ? notesFolder.split("/").pop() || notesFolder : "Notes";

  return (
    <div className="w-64 h-full bg-bg-secondary border-r border-border flex flex-col select-none">
      {/* Drag region */}
      <div className="h-11 shrink-0" data-tauri-drag-region></div>
      <div className="flex items-center justify-between pl-3 pr-3 pb-2 border-b border-border shrink-0">
        <Tooltip content={notesFolder || "No folder selected"}>
          <button
            onClick={handleChangeFolder}
            className="flex items-center gap-1.5 hover:bg-bg-muted rounded-md px-1 py-0.5 -ml-1 transition-colors cursor-pointer group"
          >
            <FolderIcon className="w-3.5 h-3.5 stroke-[1.5] text-text-muted group-hover:text-text transition-colors shrink-0" />
            <span className="font-medium text-base truncate max-w-28">{folderName}</span>
            <span className="text-text-muted font-medium text-2xs min-w-4.75 h-4.75 flex items-center justify-center px-1 bg-bg-muted rounded-sm mt-0.5 pt-px">
              {notes.length}
            </span>
          </button>
        </Tooltip>
        <div className="flex items-center gap-px">
          <IconButton onClick={toggleSearch} title="Search">
            {searchOpen ? (
              <SearchOffIcon className="w-4.25 h-4.25 stroke-[1.5]" />
            ) : (
              <SearchIcon className="w-4.25 h-4.25 stroke-[1.5]" />
            )}
          </IconButton>
          <DropdownMenu.Root>
            <DropdownMenu.Trigger asChild>
              <IconButton
                variant="ghost"
                title={`New Note (${mod}${isMac ? "" : "+"}N)`}
              >
                <PlusIcon className="w-5.25 h-5.25 stroke-[1.4]" />
              </IconButton>
            </DropdownMenu.Trigger>
            <DropdownMenu.Portal>
              <DropdownMenu.Content
                className="min-w-[280px] max-w-[340px] bg-bg rounded-lg shadow-lg border border-border p-1.5 z-50 animate-slide-down flex flex-col"
                align="end"
                sideOffset={4}
                style={{ maxHeight: "min(420px, calc(100vh - 120px))" }}
              >
                <DropdownMenu.Item
                  className="flex items-center gap-2.5 px-2.5 py-2 text-sm rounded-md cursor-pointer hover:bg-bg-muted outline-none data-[highlighted]:bg-bg-muted flex-none"
                  onSelect={() => createNote()}
                >
                  <AddNoteIcon className="w-4 h-4 stroke-[1.5] text-text-muted" />
                  <span>Blank Note</span>
                  <span className="ml-auto text-xs text-text-muted">{mod}{isMac ? "" : "+"}N</span>
                </DropdownMenu.Item>
                {templates.length > 0 && (
                  <>
                    <DropdownMenu.Separator className="h-px bg-border my-1 flex-none" />
                    <DropdownMenu.Label className="px-2.5 py-1 text-xs font-medium text-text-muted flex-none">
                      Templates
                    </DropdownMenu.Label>
                    <div className="px-1.5 pb-1 flex-none">
                      <input
                        type="text"
                        placeholder="Filter templates..."
                        className="w-full px-2 py-1.5 text-sm bg-bg-muted rounded-md outline-none text-text placeholder-text-muted/50 border border-border focus:border-text-muted/30"
                        onChange={(e) => {
                          const el = e.currentTarget.closest('[role="menu"]');
                          if (el) el.setAttribute("data-template-filter", e.target.value.toLowerCase());
                          // Force re-render of template items visibility
                          el?.querySelectorAll("[data-template-name]").forEach((item) => {
                            const name = item.getAttribute("data-template-name") || "";
                            (item as HTMLElement).style.display = name.includes(e.target.value.toLowerCase()) ? "" : "none";
                          });
                        }}
                        onKeyDown={(e) => e.stopPropagation()}
                        onClick={(e) => e.stopPropagation()}
                        autoFocus
                      />
                    </div>
                    <div className="overflow-y-auto flex-1 min-h-0">
                      {templates.map((template) => (
                        <DropdownMenu.Item
                          key={template.id}
                          data-template-name={template.name.toLowerCase()}
                          className="flex items-center gap-2.5 px-2.5 py-2 text-sm rounded-md cursor-pointer hover:bg-bg-muted outline-none data-[highlighted]:bg-bg-muted"
                          onSelect={() => createNoteFromTemplate(template.id)}
                        >
                          <TemplateIcon className="w-4 h-4 stroke-[1.5] text-text-muted flex-none" />
                          <span className="truncate">{template.name}</span>
                        </DropdownMenu.Item>
                      ))}
                    </div>
                  </>
                )}
              </DropdownMenu.Content>
            </DropdownMenu.Portal>
          </DropdownMenu.Root>
        </div>
      </div>
      {/* Scrollable area with search and notes */}
      <div className="flex-1 overflow-y-auto">
        {/* Search - sticky at top */}
        {searchOpen && (
          <div className="sticky top-0 z-10 px-2 pt-2 bg-bg-secondary">
            <div className="relative">
              <Input
                ref={searchInputRef}
                type="text"
                value={inputValue}
                onChange={handleSearchChange}
                onKeyDown={handleSearchKeyDown}
                placeholder="Search notes..."
                className="h-9 pr-8 text-sm"
              />
              {inputValue && !isSearching && (
                <button
                  onClick={handleClearSearch}
                  tabIndex={-1}
                  className="absolute right-2 top-1/2 -translate-y-1/2 text-text-muted hover:text-text"
                >
                  <XIcon className="w-4.5 h-4.5 stroke-[1.5]" />
                </button>
              )}
              {isSearching && (
                <div className="absolute right-2 top-1/2 -translate-y-1/2 text-text-muted">
                  <SpinnerIcon className="w-4.5 h-4.5 stroke-[1.5]" />
                </div>
              )}
            </div>
          </div>
        )}

        {/* Note list */}
        {!trashOpen && <NoteList />}

        {/* Trash panel */}
        {trashOpen && (
          <div className="flex flex-col gap-0.5 p-1.5">
            {trashItems.length === 0 ? (
              <div className="p-4 text-center text-sm text-text-muted select-none">
                Trash is empty
              </div>
            ) : (
              <>
                <div className="flex items-center justify-between px-2 py-1">
                  <span className="text-xs text-text-muted">{trashItems.length} deleted note{trashItems.length === 1 ? "" : "s"}</span>
                  <Button
                    onClick={handleEmptyTrash}
                    variant="ghost"
                    size="sm"
                    className="text-xs h-6 px-2 text-red-500 hover:text-red-600 hover:bg-red-500/10"
                  >
                    Empty Trash
                  </Button>
                </div>
                {trashItems.map((item) => (
                  <div
                    key={item.id}
                    className="group px-2.5 py-2 rounded-md hover:bg-bg-muted transition-colors"
                  >
                    <div className="flex items-center justify-between">
                      <span className="text-sm font-medium truncate flex-1">{item.title || "Untitled"}</span>
                    </div>
                    <div className="text-2xs text-text-muted mt-0.5 truncate">
                      {item.originalPath}
                    </div>
                    <div className="text-2xs text-text-muted/60 mt-0.5">
                      {formatTrashDate(item.deletedAt)}
                    </div>
                    <div className="flex items-center gap-1.5 mt-1.5 opacity-0 group-hover:opacity-100 transition-opacity">
                      <Button
                        onClick={() => handleRestore(item.id)}
                        variant="outline"
                        size="sm"
                        className="h-6 text-xs px-2"
                      >
                        Restore
                      </Button>
                      <Button
                        onClick={() => handleDeletePermanently(item.id)}
                        variant="ghost"
                        size="sm"
                        className="h-6 text-xs px-2 text-red-500 hover:text-red-600 hover:bg-red-500/10"
                      >
                        Delete
                      </Button>
                    </div>
                  </div>
                ))}
              </>
            )}
          </div>
        )}
      </div>

      {/* Trash toggle */}
      <button
        onClick={toggleTrash}
        className={`flex items-center gap-2 px-3.5 py-2 text-sm border-t border-border transition-colors select-none cursor-pointer ${
          trashOpen ? "bg-bg-muted text-text" : "text-text-muted hover:text-text hover:bg-bg-muted/50"
        }`}
      >
        <TrashIcon className="w-3.5 h-3.5 stroke-[1.5]" />
        <span className="text-xs font-medium">Trash</span>
        {trashCount > 0 && (
          <span className="text-2xs text-text-muted/60 bg-bg-muted rounded px-1 py-px ml-auto">{trashCount}</span>
        )}
      </button>

      {/* Footer with git status, commit, and settings */}
      <Footer onOpenSettings={onOpenSettings} />
    </div>
  );
}
