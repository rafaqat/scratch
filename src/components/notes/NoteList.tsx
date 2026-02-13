import { useCallback, useMemo, memo, useEffect, useRef, useState } from "react";
import { Menu, MenuItem } from "@tauri-apps/api/menu";
import { useNotes } from "../../context/NotesContext";
import {
  ListItem,
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "../ui";
import { cn, cleanTitle } from "../../lib/utils";
import { ChevronDownIcon, FolderIcon } from "../icons";
import * as notesService from "../../services/notes";
import type { Settings } from "../../types/note";
import type { NoteMetadata } from "../../types/note";

function formatDate(timestamp: number): string {
  const date = new Date(timestamp * 1000);
  const now = new Date();

  // Get start of today, yesterday, etc. (midnight local time)
  const startOfToday = new Date(
    now.getFullYear(),
    now.getMonth(),
    now.getDate()
  );
  const startOfYesterday = new Date(startOfToday.getTime() - 86400000);

  // Today: show time
  if (date >= startOfToday) {
    return date.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
  }

  // Yesterday
  if (date >= startOfYesterday) {
    return "Yesterday";
  }

  // Calculate days ago
  const daysAgo =
    Math.floor((startOfToday.getTime() - date.getTime()) / 86400000) + 1;

  // 2-6 days ago: show "X days ago"
  if (daysAgo <= 6) {
    return `${daysAgo} days ago`;
  }

  // This year: show month and day
  if (date.getFullYear() === now.getFullYear()) {
    return date.toLocaleDateString([], { month: "short", day: "numeric" });
  }

  // Different year: show full date
  return date.toLocaleDateString([], {
    month: "short",
    day: "numeric",
    year: "numeric",
  });
}

// --- Tree data structure ---

interface FolderNode {
  type: "folder";
  name: string;
  path: string;
  children: TreeNode[];
}

interface NoteNode {
  type: "note";
  note: NoteMetadata & { preview?: string };
}

type TreeNode = FolderNode | NoteNode;

function buildTree(items: Array<NoteMetadata & { preview?: string }>): TreeNode[] {
  const root: TreeNode[] = [];
  const folderMap = new Map<string, FolderNode>();

  // Helper to get or create a folder node
  function getFolder(path: string): FolderNode {
    if (folderMap.has(path)) return folderMap.get(path)!;

    const parts = path.split("/");
    const name = parts[parts.length - 1];
    const folder: FolderNode = { type: "folder", name, path, children: [] };
    folderMap.set(path, folder);

    if (parts.length > 1) {
      const parentPath = parts.slice(0, -1).join("/");
      const parent = getFolder(parentPath);
      // Only add if not already a child
      if (!parent.children.some((c) => c.type === "folder" && (c as FolderNode).path === path)) {
        parent.children.push(folder);
      }
    } else {
      // Top-level folder
      if (!root.some((c) => c.type === "folder" && (c as FolderNode).path === path)) {
        root.push(folder);
      }
    }

    return folder;
  }

  for (const item of items) {
    const parts = item.id.split("/");
    if (parts.length > 1) {
      // Has folder path
      const folderPath = parts.slice(0, -1).join("/");
      const folder = getFolder(folderPath);
      folder.children.push({ type: "note", note: item });
    } else {
      // Root-level note
      root.push({ type: "note", note: item });
    }
  }

  // Sort: folders first (alphabetical), then notes by modified desc
  function sortChildren(nodes: TreeNode[]): TreeNode[] {
    const folders = nodes.filter((n): n is FolderNode => n.type === "folder");
    const notes = nodes.filter((n): n is NoteNode => n.type === "note");

    folders.sort((a, b) => a.name.localeCompare(b.name));
    notes.sort((a, b) => b.note.modified - a.note.modified);

    // Recursively sort folder children
    for (const folder of folders) {
      folder.children = sortChildren(folder.children);
    }

    return [...folders, ...notes];
  }

  return sortChildren(root);
}

// --- Memoized components ---

interface NoteItemProps {
  id: string;
  title: string;
  preview?: string;
  modified: number;
  isSelected: boolean;
  isPinned: boolean;
  depth: number;
  onSelect: (id: string) => void;
  onContextMenu: (e: React.MouseEvent, id: string) => void;
}

const NoteItem = memo(function NoteItem({
  id,
  title,
  preview,
  modified,
  isSelected,
  isPinned,
  depth,
  onSelect,
  onContextMenu,
}: NoteItemProps) {
  const handleClick = useCallback(() => onSelect(id), [onSelect, id]);
  const handleContextMenu = useCallback(
    (e: React.MouseEvent) => onContextMenu(e, id),
    [onContextMenu, id]
  );

  return (
    <div style={{ paddingLeft: depth * 12 }}>
      <ListItem
        title={cleanTitle(title)}
        subtitle={preview}
        meta={formatDate(modified)}
        isSelected={isSelected}
        isPinned={isPinned}
        onClick={handleClick}
        onContextMenu={handleContextMenu}
      />
    </div>
  );
});

interface FolderItemProps {
  folder: FolderNode;
  depth: number;
  expanded: Set<string>;
  toggleFolder: (path: string) => void;
  selectedNoteId: string | null;
  pinnedIds: Set<string>;
  onSelect: (id: string) => void;
  onContextMenu: (e: React.MouseEvent, id: string) => void;
}

const FolderItem = memo(function FolderItem({
  folder,
  depth,
  expanded,
  toggleFolder,
  selectedNoteId,
  pinnedIds,
  onSelect,
  onContextMenu,
}: FolderItemProps) {
  const isExpanded = expanded.has(folder.path);
  const noteCount = countNotes(folder);

  return (
    <div>
      <button
        onClick={() => toggleFolder(folder.path)}
        className={cn(
          "w-full flex items-center gap-1 px-2.5 py-1.5 text-sm rounded-md",
          "hover:bg-bg-muted transition-colors cursor-pointer select-none",
          "text-text-muted hover:text-text"
        )}
        style={{ paddingLeft: depth * 12 + 10 }}
      >
        <ChevronDownIcon
          className={cn(
            "w-3 h-3 stroke-[2] shrink-0 transition-transform duration-150",
            !isExpanded && "-rotate-90"
          )}
        />
        <FolderIcon className="w-3.5 h-3.5 stroke-[1.5] shrink-0 opacity-60" />
        <span className="truncate font-medium text-xs">{folder.name}</span>
        <span className="text-2xs text-text-muted/50 ml-auto shrink-0">{noteCount}</span>
      </button>
      {isExpanded && (
        <div>
          {folder.children.map((child) =>
            child.type === "folder" ? (
              <FolderItem
                key={(child as FolderNode).path}
                folder={child as FolderNode}
                depth={depth + 1}
                expanded={expanded}
                toggleFolder={toggleFolder}
                selectedNoteId={selectedNoteId}
                pinnedIds={pinnedIds}
                onSelect={onSelect}
                onContextMenu={onContextMenu}
              />
            ) : (
              <NoteItem
                key={(child as NoteNode).note.id}
                id={(child as NoteNode).note.id}
                title={(child as NoteNode).note.title}
                preview={(child as NoteNode).note.preview}
                modified={(child as NoteNode).note.modified}
                isSelected={selectedNoteId === (child as NoteNode).note.id}
                isPinned={pinnedIds.has((child as NoteNode).note.id)}
                depth={depth + 1}
                onSelect={onSelect}
                onContextMenu={onContextMenu}
              />
            )
          )}
        </div>
      )}
    </div>
  );
});

function countNotes(folder: FolderNode): number {
  let count = 0;
  for (const child of folder.children) {
    if (child.type === "note") count++;
    else count += countNotes(child as FolderNode);
  }
  return count;
}

// --- Main component ---

export function NoteList() {
  const {
    notes,
    selectedNoteId,
    selectNote,
    deleteNote,
    duplicateNote,
    pinNote,
    unpinNote,
    isLoading,
    searchQuery,
    searchResults,
  } = useNotes();

  const [deleteDialogOpen, setDeleteDialogOpen] = useState(false);
  const [noteToDelete, setNoteToDelete] = useState<string | null>(null);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [expandedFolders, setExpandedFolders] = useState<Set<string>>(new Set());
  const containerRef = useRef<HTMLDivElement>(null);

  // Load settings when notes change
  useEffect(() => {
    notesService
      .getSettings()
      .then(setSettings)
      .catch((error) => {
        console.error("Failed to load settings:", error);
      });
  }, [notes]);

  // Auto-expand folder containing selected note
  useEffect(() => {
    if (selectedNoteId && selectedNoteId.includes("/")) {
      const parts = selectedNoteId.split("/");
      const paths: string[] = [];
      for (let i = 1; i < parts.length; i++) {
        paths.push(parts.slice(0, i).join("/"));
      }
      setExpandedFolders((prev) => {
        const next = new Set(prev);
        let changed = false;
        for (const p of paths) {
          if (!next.has(p)) {
            next.add(p);
            changed = true;
          }
        }
        return changed ? next : prev;
      });
    }
  }, [selectedNoteId]);

  // Calculate pinned IDs set for efficient lookup
  const pinnedIds = useMemo(
    () => new Set(settings?.pinnedNoteIds || []),
    [settings]
  );

  const toggleFolder = useCallback((path: string) => {
    setExpandedFolders((prev) => {
      const next = new Set(prev);
      if (next.has(path)) {
        next.delete(path);
      } else {
        next.add(path);
      }
      return next;
    });
  }, []);

  const handleDeleteConfirm = useCallback(async () => {
    if (noteToDelete) {
      try {
        await deleteNote(noteToDelete);
        setNoteToDelete(null);
        setDeleteDialogOpen(false);
      } catch (error) {
        console.error("Failed to delete note:", error);
      }
    }
  }, [noteToDelete, deleteNote]);

  const handleContextMenu = useCallback(
    async (e: React.MouseEvent, noteId: string) => {
      e.preventDefault();
      const isPinned = pinnedIds.has(noteId);

      const menu = await Menu.new({
        items: [
          await MenuItem.new({
            text: isPinned ? "Unpin" : "Pin",
            action: async () => {
              try {
                await (isPinned ? unpinNote(noteId) : pinNote(noteId));
                // Refresh settings after pin/unpin
                const newSettings = await notesService.getSettings();
                setSettings(newSettings);
              } catch (error) {
                console.error("Failed to pin/unpin note:", error);
              }
            },
          }),
          await MenuItem.new({
            text: "Duplicate",
            action: () => duplicateNote(noteId),
          }),
          await MenuItem.new({
            text: "Delete",
            action: () => {
              setNoteToDelete(noteId);
              setDeleteDialogOpen(true);
            },
          }),
        ],
      });

      await menu.popup();
    },
    [pinnedIds, pinNote, unpinNote, duplicateNote]
  );

  // Memoize display items
  const displayItems = useMemo(() => {
    if (searchQuery.trim()) {
      return searchResults.map((r) => ({
        id: r.id,
        title: r.title,
        preview: r.preview,
        modified: r.modified,
      }));
    }
    return notes;
  }, [searchQuery, searchResults, notes]);

  // Build tree (only when not searching - search shows flat results)
  const isSearching = searchQuery.trim().length > 0;
  const tree = useMemo(() => {
    if (isSearching) return null;
    return buildTree(displayItems);
  }, [displayItems, isSearching]);

  // Listen for focus request from editor (when Escape is pressed)
  useEffect(() => {
    const handleFocusNoteList = () => {
      containerRef.current?.focus();
    };

    window.addEventListener("focus-note-list", handleFocusNoteList);
    return () =>
      window.removeEventListener("focus-note-list", handleFocusNoteList);
  }, []);

  if (isLoading && notes.length === 0) {
    return (
      <div className="p-4 text-center text-text-muted select-none">
        Loading...
      </div>
    );
  }

  if (isSearching && displayItems.length === 0) {
    return (
      <div className="p-4 text-center text-sm text-text-muted select-none">
        No results found
      </div>
    );
  }

  if (displayItems.length === 0) {
    return (
      <div className="p-4 text-center text-sm text-text-muted select-none">
        No notes yet
      </div>
    );
  }

  // Search mode: flat list
  if (isSearching) {
    return (
      <>
        <div
          ref={containerRef}
          tabIndex={0}
          className="flex flex-col gap-1 p-1.5 outline-none"
        >
          {displayItems.map((item) => (
            <NoteItem
              key={item.id}
              id={item.id}
              title={item.title}
              preview={item.preview}
              modified={item.modified}
              isSelected={selectedNoteId === item.id}
              isPinned={pinnedIds.has(item.id)}
              depth={0}
              onSelect={selectNote}
              onContextMenu={handleContextMenu}
            />
          ))}
        </div>
        <DeleteDialog
          open={deleteDialogOpen}
          onOpenChange={setDeleteDialogOpen}
          onConfirm={handleDeleteConfirm}
        />
      </>
    );
  }

  // Tree mode: folders + notes
  return (
    <>
      <div
        ref={containerRef}
        tabIndex={0}
        className="flex flex-col gap-0.5 p-1.5 outline-none"
      >
        {tree!.map((node) =>
          node.type === "folder" ? (
            <FolderItem
              key={(node as FolderNode).path}
              folder={node as FolderNode}
              depth={0}
              expanded={expandedFolders}
              toggleFolder={toggleFolder}
              selectedNoteId={selectedNoteId}
              pinnedIds={pinnedIds}
              onSelect={selectNote}
              onContextMenu={handleContextMenu}
            />
          ) : (
            <NoteItem
              key={(node as NoteNode).note.id}
              id={(node as NoteNode).note.id}
              title={(node as NoteNode).note.title}
              preview={(node as NoteNode).note.preview}
              modified={(node as NoteNode).note.modified}
              isSelected={selectedNoteId === (node as NoteNode).note.id}
              isPinned={pinnedIds.has((node as NoteNode).note.id)}
              depth={0}
              onSelect={selectNote}
              onContextMenu={handleContextMenu}
            />
          )
        )}
      </div>
      <DeleteDialog
        open={deleteDialogOpen}
        onOpenChange={setDeleteDialogOpen}
        onConfirm={handleDeleteConfirm}
      />
    </>
  );
}

// Extracted delete dialog
function DeleteDialog({
  open,
  onOpenChange,
  onConfirm,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onConfirm: () => void;
}) {
  return (
    <AlertDialog open={open} onOpenChange={onOpenChange}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Delete note?</AlertDialogTitle>
          <AlertDialogDescription>
            This will permanently delete the note and all its content. This
            action cannot be undone.
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>Cancel</AlertDialogCancel>
          <AlertDialogAction onClick={onConfirm}>
            Delete
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
