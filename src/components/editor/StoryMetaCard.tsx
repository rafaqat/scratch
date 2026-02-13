import { useCallback, useMemo } from "react";
import { type StoryFrontmatter, STORY_STATUSES } from "../../lib/frontmatter";
import { cn } from "../../lib/utils";

interface StoryMetaCardProps {
  frontmatter: StoryFrontmatter;
  onChange: (updated: StoryFrontmatter) => void;
}

/* ─── Status config ─── */

const STATUS_CONFIG: Record<string, {
  dot: string;
  activeBg: string;
  activeText: string;
  activeBorder: string;
}> = {
  Backlog: {
    dot: "bg-neutral-400",
    activeBg: "bg-neutral-200 dark:bg-neutral-700",
    activeText: "text-neutral-700 dark:text-neutral-200",
    activeBorder: "border-neutral-400/50",
  },
  Ready: {
    dot: "bg-blue-400",
    activeBg: "bg-blue-100 dark:bg-blue-900/40",
    activeText: "text-blue-700 dark:text-blue-300",
    activeBorder: "border-blue-400/50",
  },
  "In Progress": {
    dot: "bg-amber-400",
    activeBg: "bg-amber-100 dark:bg-amber-900/40",
    activeText: "text-amber-700 dark:text-amber-300",
    activeBorder: "border-amber-400/50",
  },
  "In Review": {
    dot: "bg-purple-400",
    activeBg: "bg-purple-100 dark:bg-purple-900/40",
    activeText: "text-purple-700 dark:text-purple-300",
    activeBorder: "border-purple-400/50",
  },
  Done: {
    dot: "bg-emerald-400",
    activeBg: "bg-emerald-100 dark:bg-emerald-900/40",
    activeText: "text-emerald-700 dark:text-emerald-300",
    activeBorder: "border-emerald-400/50",
  },
  Blocked: {
    dot: "bg-red-400",
    activeBg: "bg-red-100 dark:bg-red-900/40",
    activeText: "text-red-700 dark:text-red-300",
    activeBorder: "border-red-400/50",
  },
};

// Short labels for pipeline display
const STATUS_SHORT: Record<string, string> = {
  Backlog: "BACKLOG",
  Ready: "READY",
  "In Progress": "PROGRESS",
  "In Review": "REVIEW",
  Done: "DONE",
  Blocked: "BLOCKED",
};

/* ─── Kanban Pipeline ─── */

function KanbanPipeline({
  status,
  onChangeStatus,
}: {
  status: string;
  onChangeStatus: (s: string) => void;
}) {
  const currentIdx = STORY_STATUSES.indexOf(status);

  return (
    <div className="flex items-center gap-0.5">
      {STORY_STATUSES.map((s, i) => {
        const config = STATUS_CONFIG[s] || STATUS_CONFIG.Backlog;
        const isActive = s === status;
        const isPast = i < currentIdx && status !== "Blocked";

        return (
          <button
            key={s}
            onClick={() => onChangeStatus(s)}
            title={s}
            className={cn(
              "relative px-2 py-1 text-[9px] font-bold uppercase tracking-[0.1em] rounded",
              "transition-all duration-150 cursor-pointer select-none border",
              isActive
                ? cn(config.activeBg, config.activeText, config.activeBorder)
                : isPast
                  ? "bg-neutral-200/60 dark:bg-neutral-700/40 text-neutral-400 dark:text-neutral-500 border-transparent"
                  : "bg-transparent text-neutral-300 dark:text-neutral-600 border-transparent hover:bg-neutral-100 dark:hover:bg-neutral-700/30 hover:text-neutral-500 dark:hover:text-neutral-400",
            )}
          >
            {STATUS_SHORT[s] || s.toUpperCase()}
          </button>
        );
      })}
    </div>
  );
}

/* ─── Label ─── */

function Label({ children }: { children: React.ReactNode }) {
  return (
    <span className="text-[10px] font-semibold uppercase tracking-[0.1em] text-neutral-400 dark:text-neutral-500 shrink-0">
      {children}
    </span>
  );
}

/* ─── Value ─── */

function Value({ children, mono }: { children: React.ReactNode; mono?: boolean }) {
  return (
    <span className={cn(
      "text-[11px] font-medium text-neutral-600 dark:text-neutral-400",
      mono && "tabular-nums"
    )}>
      {children}
    </span>
  );
}

/* ─── Main Card ─── */

export function StoryMetaCard({ frontmatter, onChange }: StoryMetaCardProps) {
  const today = useMemo(() => new Date().toISOString().split("T")[0], []);

  const handleStatusChange = useCallback(
    (newStatus: string) => {
      const updates: Partial<StoryFrontmatter> = { status: newStatus };

      // Auto-set started_at when entering In Progress (or beyond) for the first time
      if (
        !frontmatter.started_at &&
        ["In Progress", "In Review", "Done"].includes(newStatus)
      ) {
        updates.started_at = today;
      }

      // Auto-set completed_at when moving to Done
      if (newStatus === "Done") {
        updates.completed_at = today;
      }

      // Clear completed_at if moving back from Done
      if (newStatus !== "Done" && frontmatter.completed_at) {
        updates.completed_at = "";
      }

      onChange({ ...frontmatter, ...updates });
    },
    [frontmatter, onChange, today]
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

        {/* Kanban Pipeline */}
        <div className="mb-3">
          <KanbanPipeline status={frontmatter.status} onChangeStatus={handleStatusChange} />
        </div>

        {/* Row: Points + Tags */}
        <div className="flex items-baseline gap-6 mb-1.5 flex-wrap">
          {frontmatter.estimate_points !== undefined && (
            <div className="flex items-baseline gap-1.5">
              <Label>Points:</Label>
              <span className="text-sm font-bold text-neutral-900 dark:text-neutral-100 tabular-nums">
                {frontmatter.estimate_points}
              </span>
            </div>
          )}
          {frontmatter.tags && frontmatter.tags.length > 0 && (
            <div className="flex items-baseline gap-1.5">
              <Label>Tags:</Label>
              <Value>{frontmatter.tags.join(", ")}</Value>
            </div>
          )}
        </div>

        {/* Row: Owner */}
        <div className="flex items-baseline gap-1.5 mb-1.5">
          <Label>Owner:</Label>
          <Value>{frontmatter.owner || "Unassigned"}</Value>
        </div>

        {/* Row: Dates - auto-populated by state changes */}
        <div className="flex items-baseline gap-4 mb-3 flex-wrap">
          <div className="flex items-baseline gap-1.5">
            <Label>Created:</Label>
            <Value mono>{frontmatter.created_at || "—"}</Value>
          </div>
          <div className="flex items-baseline gap-1.5">
            <Label>Started:</Label>
            <Value mono>{frontmatter.started_at || "—"}</Value>
          </div>
          <div className="flex items-baseline gap-1.5">
            <Label>Done:</Label>
            <Value mono>{frontmatter.completed_at || "—"}</Value>
          </div>
        </div>

        {/* Divider */}
        <div className="h-px bg-neutral-300 dark:bg-neutral-600 mb-3" />

        {/* Row: Version + Commits + Links */}
        <div className="flex items-baseline gap-4 mb-1.5 flex-wrap">
          <div className="flex items-baseline gap-1.5">
            <Label>Version:</Label>
            <Value>{frontmatter.target_version || "—"}</Value>
          </div>
          <div className="flex items-baseline gap-1.5">
            <Label>Commits:</Label>
            <Value mono>
              {frontmatter.commits && frontmatter.commits.length > 0
                ? frontmatter.commits.length
                : "—"}
            </Value>
          </div>
          {hasLinks && (
            <div className="flex items-baseline gap-1.5">
              <Label>Links:</Label>
              <Value>
                {blocks.length > 0 && `blocks ${blocks.length}`}
                {blocks.length > 0 && blockedBy.length > 0 && " · "}
                {blockedBy.length > 0 && `blocked ${blockedBy.length}`}
              </Value>
            </div>
          )}
        </div>

        {/* Row: Epic */}
        {epic && (
          <div className="flex items-baseline gap-1.5">
            <Label>Epic:</Label>
            <Value>{epic}</Value>
          </div>
        )}
      </div>
    </div>
  );
}
