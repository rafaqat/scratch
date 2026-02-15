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
import { ChevronDownIcon, FolderIcon, PinIcon } from "../icons";
import * as notesService from "../../services/notes";
import type { Settings } from "../../types/note";
import type { NoteMetadata } from "../../types/note";

// Drag data type key
const DRAG_NOTE_TYPE = "application/x-scratch-note-id";

// Persistence key for expanded folders
const EXPANDED_FOLDERS_KEY = "scratch-expanded-folders";

function loadExpandedFolders(): Set<string> {
  try {
    const stored = localStorage.getItem(EXPANDED_FOLDERS_KEY);
    if (stored) return new Set(JSON.parse(stored));
  } catch { /* ignore */ }
  return new Set<string>();
}

function saveExpandedFolders(folders: Set<string>) {
  try {
    localStorage.setItem(EXPANDED_FOLDERS_KEY, JSON.stringify([...folders]));
  } catch { /* ignore */ }
}

function formatDate(timestamp: number): string {
  const date = new Date(timestamp * 1000);
  const now = new Date();

  const startOfToday = new Date(
    now.getFullYear(),
    now.getMonth(),
    now.getDate()
  );
  const startOfYesterday = new Date(startOfToday.getTime() - 86400000);

  if (date >= startOfToday) {
    return date.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
  }
  if (date >= startOfYesterday) {
    return "Yesterday";
  }
  const daysAgo =
    Math.floor((startOfToday.getTime() - date.getTime()) / 86400000) + 1;
  if (daysAgo <= 6) {
    return `${daysAgo} days ago`;
  }
  if (date.getFullYear() === now.getFullYear()) {
    return date.toLocaleDateString([], { month: "short", day: "numeric" });
  }
  return date.toLocaleDateString([], {
    month: "short",
    day: "numeric",
    year: "numeric",
  });
}

// Get the folder portion of a note ID (or "." for root)
function noteFolderPath(noteId: string): string {
  const idx = noteId.lastIndexOf("/");
  return idx >= 0 ? noteId.slice(0, idx) : ".";
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

  function getFolder(path: string): FolderNode {
    if (folderMap.has(path)) return folderMap.get(path)!;

    const parts = path.split("/");
    const name = parts[parts.length - 1];
    const folder: FolderNode = { type: "folder", name, path, children: [] };
    folderMap.set(path, folder);

    if (parts.length > 1) {
      const parentPath = parts.slice(0, -1).join("/");
      const parent = getFolder(parentPath);
      if (!parent.children.some((c) => c.type === "folder" && (c as FolderNode).path === path)) {
        parent.children.push(folder);
      }
    } else {
      if (!root.some((c) => c.type === "folder" && (c as FolderNode).path === path)) {
        root.push(folder);
      }
    }

    return folder;
  }

  for (const item of items) {
    const parts = item.id.split("/");
    if (parts.length > 1) {
      const folderPath = parts.slice(0, -1).join("/");
      const folder = getFolder(folderPath);
      folder.children.push({ type: "note", note: item });
    } else {
      root.push({ type: "note", note: item });
    }
  }

  function sortChildren(nodes: TreeNode[]): TreeNode[] {
    const folders = nodes.filter((n): n is FolderNode => n.type === "folder");
    const notes = nodes.filter((n): n is NoteNode => n.type === "note");

    folders.sort((a, b) => a.name.localeCompare(b.name));
    notes.sort((a, b) => b.note.modified - a.note.modified);

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
  icon?: string;
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
  icon,
  onSelect,
  onContextMenu,
}: NoteItemProps) {
  const handleClick = useCallback(() => onSelect(id), [onSelect, id]);
  const handleContextMenu = useCallback(
    (e: React.MouseEvent) => onContextMenu(e, id),
    [onContextMenu, id]
  );

  const displayTitle = icon ? `${icon} ${cleanTitle(title)}` : cleanTitle(title);

  return (
    <div
      style={{ paddingLeft: depth * 12 }}
      draggable
      onDragStart={(e) => {
        e.dataTransfer.setData(DRAG_NOTE_TYPE, id);
        e.dataTransfer.effectAllowed = "move";
      }}
    >
      <ListItem
        title={displayTitle}
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
  onNoteContextMenu: (e: React.MouseEvent, id: string) => void;
  onFolderContextMenu: (e: React.MouseEvent, folderPath: string) => void;
  onDrop: (noteId: string, targetFolder: string) => void;
  renamingFolder: string | null;
  renamingValue: string;
  onRenamingChange: (value: string) => void;
  onRenamingSubmit: () => void;
  onRenamingCancel: () => void;
}

const FolderItem = memo(function FolderItem({
  folder,
  depth,
  expanded,
  toggleFolder,
  selectedNoteId,
  pinnedIds,
  onSelect,
  onNoteContextMenu,
  onFolderContextMenu,
  onDrop,
  renamingFolder,
  renamingValue,
  onRenamingChange,
  onRenamingSubmit,
  onRenamingCancel,
}: FolderItemProps) {
  const isExpanded = expanded.has(folder.path);
  const noteCount = countNotes(folder);
  const isRenaming = renamingFolder === folder.path;
  const [isDragOver, setIsDragOver] = useState(false);

  return (
    <div>
      <button
        onClick={() => toggleFolder(folder.path)}
        onContextMenu={(e) => {
          e.preventDefault();
          onFolderContextMenu(e, folder.path);
        }}
        onDragOver={(e) => {
          if (e.dataTransfer.types.includes(DRAG_NOTE_TYPE)) {
            e.preventDefault();
            e.dataTransfer.dropEffect = "move";
            setIsDragOver(true);
          }
        }}
        onDragLeave={() => setIsDragOver(false)}
        onDrop={(e) => {
          e.preventDefault();
          setIsDragOver(false);
          const noteId = e.dataTransfer.getData(DRAG_NOTE_TYPE);
          if (noteId) {
            onDrop(noteId, folder.path);
          }
        }}
        className={cn(
          "w-full flex items-center gap-1 px-2.5 py-1.5 text-sm rounded-md",
          "hover:bg-bg-muted transition-colors cursor-pointer select-none",
          "text-text-muted hover:text-text",
          isDragOver && "bg-accent/15 ring-1 ring-accent/40"
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
        {isRenaming ? (
          <input
            className="flex-1 min-w-0 text-xs font-medium bg-bg-surface border border-border rounded px-1 py-0 outline-none"
            value={renamingValue}
            onChange={(e) => onRenamingChange(e.target.value)}
            onKeyDown={(e) => {
              e.stopPropagation();
              if (e.key === "Enter") onRenamingSubmit();
              if (e.key === "Escape") onRenamingCancel();
            }}
            onBlur={onRenamingSubmit}
            onClick={(e) => e.stopPropagation()}
            autoFocus
          />
        ) : (
          <span className="truncate font-medium text-xs">{folder.name}</span>
        )}
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
                onNoteContextMenu={onNoteContextMenu}
                onFolderContextMenu={onFolderContextMenu}
                onDrop={onDrop}
                renamingFolder={renamingFolder}
                renamingValue={renamingValue}
                onRenamingChange={onRenamingChange}
                onRenamingSubmit={onRenamingSubmit}
                onRenamingCancel={onRenamingCancel}
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
                icon={(child as NoteNode).note.icon}
                onSelect={onSelect}
                onContextMenu={onNoteContextMenu}
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

// --- Pinned Notes Section ---

function PinnedSection({
  notes,
  pinnedIds,
  selectedNoteId,
  onSelect,
  onContextMenu,
}: {
  notes: Array<NoteMetadata & { preview?: string }>;
  pinnedIds: Set<string>;
  selectedNoteId: string | null;
  onSelect: (id: string) => void;
  onContextMenu: (e: React.MouseEvent, id: string) => void;
}) {
  const pinnedNotes = useMemo(
    () => notes.filter((n) => pinnedIds.has(n.id)),
    [notes, pinnedIds]
  );

  if (pinnedNotes.length === 0) return null;

  return (
    <div className="mb-1">
      <div className="flex items-center gap-1 px-3 py-1 text-2xs font-medium text-text-muted/50 uppercase tracking-wider select-none">
        <PinIcon className="w-3 h-3 stroke-[1.5] opacity-50" />
        Pinned
      </div>
      {pinnedNotes.map((item) => (
        <NoteItem
          key={item.id}
          id={item.id}
          title={item.title}
          preview={item.preview}
          modified={item.modified}
          isSelected={selectedNoteId === item.id}
          isPinned={true}
          depth={0}
          icon={item.icon}
          onSelect={onSelect}
          onContextMenu={onContextMenu}
        />
      ))}
    </div>
  );
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
    refreshNotes,
  } = useNotes();

  const [deleteDialogOpen, setDeleteDialogOpen] = useState(false);
  const [noteToDelete, setNoteToDelete] = useState<string | null>(null);
  const [deleteFolderDialogOpen, setDeleteFolderDialogOpen] = useState(false);
  const [folderToDelete, setFolderToDelete] = useState<string | null>(null);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [expandedFolders, setExpandedFolders] = useState<Set<string>>(loadExpandedFolders);
  const [renamingFolder, setRenamingFolder] = useState<string | null>(null);
  const [renamingValue, setRenamingValue] = useState("");
  const [rootDragOver, setRootDragOver] = useState(false);
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

  // Persist expanded folders to localStorage
  useEffect(() => {
    saveExpandedFolders(expandedFolders);
  }, [expandedFolders]);

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

  const handleDeleteFolderConfirm = useCallback(async () => {
    if (folderToDelete) {
      try {
        await notesService.deleteFolder(folderToDelete);
        await refreshNotes();
        setFolderToDelete(null);
        setDeleteFolderDialogOpen(false);
      } catch (error) {
        console.error("Failed to delete folder:", error);
      }
    }
  }, [folderToDelete, refreshNotes]);

  // Drag and drop: move note to folder
  const handleDropNote = useCallback(
    async (noteId: string, targetFolder: string) => {
      // Don't move if already in that folder
      const currentFolder = noteFolderPath(noteId);
      if (currentFolder === targetFolder) return;

      try {
        const movedNote = await notesService.moveNote(noteId, targetFolder);
        await refreshNotes();
        selectNote(movedNote.id);
      } catch (error) {
        console.error("Failed to move note:", error);
      }
    },
    [refreshNotes, selectNote]
  );

  const handleNoteContextMenu = useCallback(
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

  const handleFolderContextMenu = useCallback(
    async (e: React.MouseEvent, folderPath: string) => {
      e.preventDefault();

      const menu = await Menu.new({
        items: [
          await MenuItem.new({
            text: "New Note",
            action: async () => {
              try {
                const note = await notesService.createNoteInFolder(folderPath);
                await refreshNotes();
                selectNote(note.id);
              } catch (error) {
                console.error("Failed to create note in folder:", error);
              }
            },
          }),
          await MenuItem.new({
            text: "New Subfolder",
            action: async () => {
              try {
                let name = "new-folder";
                let counter = 1;
                const allNoteIds = notes.map((n) => n.id);
                while (allNoteIds.some((id) => id.startsWith(`${folderPath}/${name}/`))) {
                  name = `new-folder-${counter}`;
                  counter++;
                }
                const createdPath = await notesService.createFolder(`${folderPath}/${name}`);
                setExpandedFolders((prev) => {
                  const next = new Set(prev);
                  next.add(folderPath);
                  return next;
                });
                await refreshNotes();
                setRenamingFolder(createdPath);
                setRenamingValue(name);
              } catch (error) {
                console.error("Failed to create subfolder:", error);
              }
            },
          }),
          await MenuItem.new({
            text: "Rename",
            action: () => {
              const name = folderPath.split("/").pop() || folderPath;
              setRenamingFolder(folderPath);
              setRenamingValue(name);
            },
          }),
          await MenuItem.new({
            text: "Delete",
            action: () => {
              setFolderToDelete(folderPath);
              setDeleteFolderDialogOpen(true);
            },
          }),
        ],
      });

      await menu.popup();
    },
    [notes, refreshNotes, selectNote]
  );

  const handleRenamingSubmit = useCallback(async () => {
    if (!renamingFolder || !renamingValue.trim()) {
      setRenamingFolder(null);
      return;
    }
    const currentName = renamingFolder.split("/").pop() || "";
    if (renamingValue.trim() === currentName) {
      setRenamingFolder(null);
      return;
    }
    try {
      await notesService.renameFolder(renamingFolder, renamingValue.trim());
      await refreshNotes();
    } catch (error) {
      console.error("Failed to rename folder:", error);
    }
    setRenamingFolder(null);
  }, [renamingFolder, renamingValue, refreshNotes]);

  const handleRenamingCancel = useCallback(() => {
    setRenamingFolder(null);
  }, []);

  const displayItems = useMemo(() => {
    if (searchQuery.trim()) {
      const noteMap = new Map(notes.map((n) => [n.id, n]));
      return searchResults.map((r) => ({
        id: r.id,
        title: r.title,
        preview: r.preview,
        modified: r.modified,
        icon: noteMap.get(r.id)?.icon,
      }));
    }
    return notes;
  }, [searchQuery, searchResults, notes]);

  const isSearching = searchQuery.trim().length > 0;
  const tree = useMemo(() => {
    if (isSearching) return null;
    return buildTree(displayItems);
  }, [displayItems, isSearching]);

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

  // Search mode: flat list (no drag)
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
              icon={item.icon}
              onSelect={selectNote}
              onContextMenu={handleNoteContextMenu}
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

  // Tree mode: pinned section + folders + notes with drag-and-drop
  return (
    <>
      <div
        ref={containerRef}
        tabIndex={0}
        className={cn(
          "flex flex-col gap-0.5 p-1.5 outline-none min-h-full",
          rootDragOver && "bg-accent/5"
        )}
        onDragOver={(e) => {
          // Only accept drop on the container background (root zone)
          if (e.target === containerRef.current && e.dataTransfer.types.includes(DRAG_NOTE_TYPE)) {
            e.preventDefault();
            e.dataTransfer.dropEffect = "move";
            setRootDragOver(true);
          }
        }}
        onDragLeave={(e) => {
          if (e.target === containerRef.current) setRootDragOver(false);
        }}
        onDrop={(e) => {
          if (e.target === containerRef.current) {
            e.preventDefault();
            setRootDragOver(false);
            const noteId = e.dataTransfer.getData(DRAG_NOTE_TYPE);
            if (noteId) {
              handleDropNote(noteId, ".");
            }
          }
        }}
      >
        {/* Pinned notes section */}
        <PinnedSection
          notes={displayItems}
          pinnedIds={pinnedIds}
          selectedNoteId={selectedNoteId}
          onSelect={selectNote}
          onContextMenu={handleNoteContextMenu}
        />

        {/* Folder tree */}
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
              onNoteContextMenu={handleNoteContextMenu}
              onFolderContextMenu={handleFolderContextMenu}
              onDrop={handleDropNote}
              renamingFolder={renamingFolder}
              renamingValue={renamingValue}
              onRenamingChange={setRenamingValue}
              onRenamingSubmit={handleRenamingSubmit}
              onRenamingCancel={handleRenamingCancel}
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
              icon={(node as NoteNode).note.icon}
              onSelect={selectNote}
              onContextMenu={handleNoteContextMenu}
            />
          )
        )}
      </div>
      <DeleteDialog
        open={deleteDialogOpen}
        onOpenChange={setDeleteDialogOpen}
        onConfirm={handleDeleteConfirm}
      />
      <DeleteFolderDialog
        open={deleteFolderDialogOpen}
        onOpenChange={setDeleteFolderDialogOpen}
        onConfirm={handleDeleteFolderConfirm}
        folderName={folderToDelete?.split("/").pop() || ""}
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

function DeleteFolderDialog({
  open,
  onOpenChange,
  onConfirm,
  folderName,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onConfirm: () => void;
  folderName: string;
}) {
  return (
    <AlertDialog open={open} onOpenChange={onOpenChange}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Delete folder "{folderName}"?</AlertDialogTitle>
          <AlertDialogDescription>
            This will permanently delete the folder and all notes inside it.
            This action cannot be undone.
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
