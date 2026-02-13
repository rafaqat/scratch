import { useState, useRef, useEffect, useCallback, useMemo } from "react";
import { type StoryFrontmatter, STORY_STATUSES } from "../../lib/frontmatter";
import { cn } from "../../lib/utils";

interface StoryMetaCardProps {
  frontmatter: StoryFrontmatter;
  onChange: (updated: StoryFrontmatter) => void;
}

/* ─── Status colors ─── */

const STATUS_COLORS: Record<string, { dot: string; text: string }> = {
  Backlog: { dot: "bg-neutral-400", text: "text-neutral-400 dark:text-neutral-400" },
  Ready: { dot: "bg-blue-400", text: "text-blue-400 dark:text-blue-400" },
  "In Progress": { dot: "bg-amber-400", text: "text-amber-400 dark:text-amber-400" },
  "In Review": { dot: "bg-purple-400", text: "text-purple-400 dark:text-purple-400" },
  Done: { dot: "bg-emerald-400", text: "text-emerald-400 dark:text-emerald-400" },
  Blocked: { dot: "bg-red-400", text: "text-red-400 dark:text-red-400" },
};

/* ─── Status Dropdown ─── */

function StatusDropdown({
  status,
  onChangeStatus,
}: {
  status: string;
  onChangeStatus: (s: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const [animating, setAnimating] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const handleClick = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) {
        setAnimating(false);
        setTimeout(() => setOpen(false), 120);
      }
    };
    const handleEsc = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        setAnimating(false);
        setTimeout(() => setOpen(false), 120);
      }
    };
    document.addEventListener("mousedown", handleClick);
    document.addEventListener("keydown", handleEsc);
    return () => {
      document.removeEventListener("mousedown", handleClick);
      document.removeEventListener("keydown", handleEsc);
    };
  }, [open]);

  const handleOpen = () => {
    setOpen(true);
    requestAnimationFrame(() => setAnimating(true));
  };

  const handleSelect = (s: string) => {
    onChangeStatus(s);
    setAnimating(false);
    setTimeout(() => setOpen(false), 120);
  };

  const colors = STATUS_COLORS[status] || STATUS_COLORS.Backlog;

  return (
    <div ref={ref} className="relative">
      <button
        onClick={open ? () => { setAnimating(false); setTimeout(() => setOpen(false), 120); } : handleOpen}
        className={cn(
          "inline-flex items-center gap-1.5 cursor-pointer select-none",
          "transition-opacity hover:opacity-80 active:scale-[0.98]"
        )}
      >
        <span className={cn(colors.text, "text-[11px] font-semibold uppercase tracking-[0.08em]")}>
          {status}
        </span>
        <svg
          className={cn("w-2.5 h-2.5 opacity-40 transition-transform duration-150", open && "rotate-180")}
          viewBox="0 0 10 10"
          fill="none"
        >
          <path d="M2.5 4L5 6.5L7.5 4" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
        </svg>
      </button>

      {open && (
        <div
          className={cn(
            "absolute top-full left-0 mt-1.5 z-50 min-w-[160px]",
            "rounded-lg overflow-hidden border",
            "bg-white dark:bg-neutral-800 border-neutral-200 dark:border-neutral-700",
            "transition-all duration-150 origin-top-left",
            animating
              ? "opacity-100 scale-100 shadow-xl translate-y-0"
              : "opacity-0 scale-95 shadow-none -translate-y-1"
          )}
        >
          <div className="p-0.5">
            {STORY_STATUSES.map((s) => {
              const c = STATUS_COLORS[s] || STATUS_COLORS.Backlog;
              const isActive = s === status;
              return (
                <button
                  key={s}
                  onClick={() => handleSelect(s)}
                  className={cn(
                    "w-full flex items-center gap-2.5 px-3 py-1.5 rounded-md text-[11px] text-left",
                    "transition-colors duration-100 cursor-pointer",
                    isActive
                      ? "bg-neutral-100 dark:bg-neutral-700"
                      : "hover:bg-neutral-50 dark:hover:bg-neutral-700/50"
                  )}
                >
                  <div className={cn("w-2 h-2 rounded-full flex-shrink-0", c.dot)} />
                  <span className={cn(
                    "flex-1 uppercase tracking-[0.06em] font-medium",
                    isActive
                      ? "text-neutral-900 dark:text-neutral-100"
                      : "text-neutral-600 dark:text-neutral-400"
                  )}>
                    {s}
                  </span>
                  {isActive && (
                    <svg className="w-3 h-3 text-neutral-400" viewBox="0 0 12 12" fill="none">
                      <path d="M2 6.5L4.5 9L10 3" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
                    </svg>
                  )}
                </button>
              );
            })}
          </div>
        </div>
      )}
    </div>
  );
}

/* ─── Label ─── */

function Label({ children }: { children: React.ReactNode }) {
  return (
    <span className="text-[10px] font-semibold uppercase tracking-[0.1em] text-neutral-400 dark:text-neutral-500">
      {children}
    </span>
  );
}

/* ─── Main Card ─── */

export function StoryMetaCard({ frontmatter, onChange }: StoryMetaCardProps) {
  const handleStatusChange = useCallback(
    (newStatus: string) => {
      const now = new Date().toISOString().split("T")[0];
      const updates: Partial<StoryFrontmatter> = { status: newStatus };
      if (newStatus === "In Progress" && !frontmatter.started_at) {
        updates.started_at = now;
      }
      if (newStatus === "Done" && !frontmatter.completed_at) {
        updates.completed_at = now;
      }
      onChange({ ...frontmatter, ...updates });
    },
    [frontmatter, onChange]
  );

  const statusColors = useMemo(
    () => STATUS_COLORS[frontmatter.status] || STATUS_COLORS.Backlog,
    [frontmatter.status]
  );

  const epic = frontmatter.links?.epic || "";
  const blocks = frontmatter.links?.blocks || [];
  const blockedBy = frontmatter.links?.blockedBy || [];
  const hasLinks = blocks.length > 0 || blockedBy.length > 0;

  return (
    <div
      className={cn(
        "mb-5 rounded-xl overflow-hidden",
        "bg-neutral-100 dark:bg-neutral-800",
        "border border-neutral-200 dark:border-neutral-700/60",
        "shadow-sm"
      )}
    >
      <div className="px-5 pt-4 pb-4">
        {/* ID */}
        <div className="text-[11px] font-mono tracking-wide text-neutral-400 dark:text-neutral-500 mb-0.5">
          {frontmatter.id}
        </div>

        {/* Title */}
        <h2 className="text-lg font-bold uppercase tracking-[0.04em] text-neutral-900 dark:text-neutral-100 leading-tight mb-3">
          {frontmatter.title}
        </h2>

        {/* Divider */}
        <div className="h-px bg-neutral-300 dark:bg-neutral-600 mb-3" />

        {/* Row 1: Status + Points */}
        <div className="flex items-baseline gap-6 mb-1.5">
          <div className="flex items-center gap-2">
            <Label>Status:</Label>
            <div className={cn("w-1.5 h-1.5 rounded-full", statusColors.dot)} />
            <StatusDropdown status={frontmatter.status} onChangeStatus={handleStatusChange} />
          </div>
          {frontmatter.estimate_points !== undefined && (
            <div className="flex items-baseline gap-2 ml-auto">
              <Label>Est. Points:</Label>
              <span className="text-sm font-bold text-neutral-900 dark:text-neutral-100 tabular-nums">
                {frontmatter.estimate_points}
              </span>
            </div>
          )}
        </div>

        {/* Row 2: Tags */}
        {frontmatter.tags && frontmatter.tags.length > 0 && (
          <div className="flex items-baseline gap-2 mb-1.5">
            <Label>Tags:</Label>
            <span className="text-[11px] font-medium uppercase tracking-[0.04em] text-neutral-600 dark:text-neutral-400">
              [{frontmatter.tags.join(", ")}]
            </span>
          </div>
        )}

        {/* Row 3: Owner */}
        <div className="flex items-baseline gap-2 mb-1.5">
          <Label>Owner:</Label>
          <span className="text-[11px] font-medium uppercase tracking-[0.04em] text-neutral-600 dark:text-neutral-400">
            {frontmatter.owner || "Unassigned"}
          </span>
        </div>

        {/* Row 4: Dates */}
        <div className="flex items-baseline gap-4 mb-3">
          <div className="flex items-baseline gap-1.5">
            <Label>Created:</Label>
            <span className="text-[11px] text-neutral-600 dark:text-neutral-400 tabular-nums">
              {frontmatter.created_at || "—"}
            </span>
          </div>
          <div className="flex items-baseline gap-1.5">
            <Label>Started:</Label>
            <span className="text-[11px] text-neutral-600 dark:text-neutral-400 tabular-nums">
              {frontmatter.started_at || "—"}
            </span>
          </div>
          <div className="flex items-baseline gap-1.5">
            <Label>Done:</Label>
            <span className="text-[11px] text-neutral-600 dark:text-neutral-400 tabular-nums">
              {frontmatter.completed_at || "—"}
            </span>
          </div>
        </div>

        {/* Divider */}
        <div className="h-px bg-neutral-300 dark:bg-neutral-600 mb-3" />

        {/* Row 5: Version + Commits + Links */}
        <div className="flex items-baseline gap-4 mb-1.5">
          <div className="flex items-baseline gap-1.5">
            <Label>Version:</Label>
            <span className="text-[11px] text-neutral-600 dark:text-neutral-400">
              {frontmatter.target_version || "—"}
            </span>
          </div>
          <div className="flex items-baseline gap-1.5">
            <Label>Commits:</Label>
            <span className="text-[11px] text-neutral-600 dark:text-neutral-400 tabular-nums">
              {frontmatter.commits && frontmatter.commits.length > 0
                ? frontmatter.commits.length
                : "—"}
            </span>
          </div>
          {hasLinks && (
            <div className="flex items-baseline gap-1.5">
              <Label>Links:</Label>
              <span className="text-[11px] text-neutral-600 dark:text-neutral-400">
                {blocks.length > 0 && `blocks ${blocks.length}`}
                {blocks.length > 0 && blockedBy.length > 0 && " · "}
                {blockedBy.length > 0 && `blocked ${blockedBy.length}`}
              </span>
            </div>
          )}
        </div>

        {/* Row 6: Epic */}
        {epic && (
          <div className="flex items-baseline gap-2">
            <Label>Epic:</Label>
            <span className="text-[11px] font-medium uppercase tracking-[0.04em] text-neutral-600 dark:text-neutral-400">
              {epic}
            </span>
          </div>
        )}
      </div>
    </div>
  );
}
