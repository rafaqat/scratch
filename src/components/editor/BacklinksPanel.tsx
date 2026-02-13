import { useState, useEffect, useCallback, useRef } from "react";
import { cn } from "../../lib/utils";
import { ChevronDownIcon, LinkIcon } from "../icons";
import * as notesService from "../../services/notes";
import type { BacklinkEntry } from "../../services/notes";

interface BacklinksPanelProps {
  noteTitle: string;
  noteId: string;
  /** Changes to this value trigger a backlinks refresh (e.g. notes list version). */
  refreshTrigger?: number;
  onNavigate: (noteId: string) => void;
}

export function BacklinksPanel({
  noteTitle,
  noteId,
  refreshTrigger,
  onNavigate,
}: BacklinksPanelProps) {
  const [backlinks, setBacklinks] = useState<BacklinkEntry[]>([]);
  const [isCollapsed, setIsCollapsed] = useState(false);
  const [isLoading, setIsLoading] = useState(true);
  // Track whether user has manually toggled collapse state
  const userToggledRef = useRef(false);
  // Track the note ID to reset user toggle on note switch
  const prevNoteIdRef = useRef(noteId);

  // Reset user toggle when switching notes
  if (prevNoteIdRef.current !== noteId) {
    prevNoteIdRef.current = noteId;
    userToggledRef.current = false;
  }

  const fetchBacklinks = useCallback(async () => {
    if (!noteTitle) {
      setBacklinks([]);
      setIsLoading(false);
      return;
    }

    try {
      const results = await notesService.getBacklinks(noteTitle);
      // Filter out self-references
      const filtered = results.filter((b) => b.noteId !== noteId);
      setBacklinks(filtered);
    } catch (err) {
      console.error("Failed to fetch backlinks:", err);
      setBacklinks([]);
    } finally {
      setIsLoading(false);
    }
  }, [noteTitle, noteId]);

  useEffect(() => {
    setIsLoading(true);
    fetchBacklinks();
  }, [fetchBacklinks, refreshTrigger]);

  // Auto-collapse/expand based on backlinks count, unless user has manually toggled
  useEffect(() => {
    if (!isLoading && !userToggledRef.current) {
      setIsCollapsed(backlinks.length === 0);
    }
  }, [backlinks.length, isLoading]);

  const handleToggle = () => {
    userToggledRef.current = true;
    setIsCollapsed(!isCollapsed);
  };

  if (isLoading) {
    return null;
  }

  return (
    <div className="border-t border-border/50 mt-8 pt-4 pb-8">
      <button
        onClick={handleToggle}
        className="flex items-center gap-1.5 text-xs font-medium text-text-muted hover:text-text transition-colors w-full group"
      >
        <ChevronDownIcon
          className={cn(
            "w-3.5 h-3.5 stroke-[2] transition-transform duration-150",
            isCollapsed && "-rotate-90",
          )}
        />
        <LinkIcon className="w-3.5 h-3.5 stroke-[1.8]" />
        <span>
          {backlinks.length} linked mention{backlinks.length !== 1 ? "s" : ""}
        </span>
      </button>

      {!isCollapsed && backlinks.length > 0 && (
        <div className="mt-3 space-y-2">
          {backlinks.map((backlink) => (
            <button
              key={backlink.noteId}
              onClick={() => onNavigate(backlink.noteId)}
              className="w-full text-left px-3 py-2 rounded-md border border-border/40 hover:border-border hover:bg-bg-muted/50 transition-colors group"
            >
              <div className="text-sm font-medium text-text group-hover:text-accent truncate">
                {backlink.noteTitle}
              </div>
              {backlink.context && (
                <div className="text-xs text-text-muted mt-0.5 line-clamp-2 leading-relaxed">
                  {backlink.context}
                </div>
              )}
            </button>
          ))}
        </div>
      )}

      {!isCollapsed && backlinks.length === 0 && (
        <p className="mt-2 text-xs text-text-muted/60 pl-5">
          No other notes link to this one yet.
        </p>
      )}
    </div>
  );
}
