import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import * as DropdownMenu from "@radix-ui/react-dropdown-menu";
import { useNotes } from "../../context/NotesContext";
import { useTheme } from "../../context/ThemeContext";
import { NoteList } from "../notes/NoteList";
import { Footer } from "./Footer";
import { IconButton, Input, Tooltip } from "../ui";
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
} from "../icons";
import { mod, isMac } from "../../lib/platform";

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
                className="min-w-[200px] bg-bg rounded-lg shadow-lg border border-border p-1.5 z-50 animate-slide-down"
                align="end"
                sideOffset={4}
              >
                <DropdownMenu.Item
                  className="flex items-center gap-2.5 px-2.5 py-2 text-sm rounded-md cursor-pointer hover:bg-bg-muted outline-none data-[highlighted]:bg-bg-muted"
                  onSelect={() => createNote()}
                >
                  <AddNoteIcon className="w-4 h-4 stroke-[1.5] text-text-muted" />
                  <span>Blank Note</span>
                  <span className="ml-auto text-xs text-text-muted">{mod}{isMac ? "" : "+"}N</span>
                </DropdownMenu.Item>
                {templates.length > 0 && (
                  <>
                    <DropdownMenu.Separator className="h-px bg-border my-1" />
                    <DropdownMenu.Label className="px-2.5 py-1 text-xs font-medium text-text-muted">
                      Templates
                    </DropdownMenu.Label>
                    {templates.map((template) => (
                      <DropdownMenu.Item
                        key={template.id}
                        className="flex items-center gap-2.5 px-2.5 py-2 text-sm rounded-md cursor-pointer hover:bg-bg-muted outline-none data-[highlighted]:bg-bg-muted"
                        onSelect={() => createNoteFromTemplate(template.id)}
                      >
                        <TemplateIcon className="w-4 h-4 stroke-[1.5] text-text-muted" />
                        <span>{template.name}</span>
                      </DropdownMenu.Item>
                    ))}
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
        <NoteList />
      </div>

      {/* Footer with git status, commit, and settings */}
      <Footer onOpenSettings={onOpenSettings} />
    </div>
  );
}
