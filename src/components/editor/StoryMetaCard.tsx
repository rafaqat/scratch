import { useState, useRef, useEffect, useCallback, useMemo } from "react";
import { type StoryFrontmatter, STORY_STATUSES } from "../../lib/frontmatter";
import { cn } from "../../lib/utils";

interface StoryMetaCardProps {
  frontmatter: StoryFrontmatter;
  onChange: (updated: StoryFrontmatter) => void;
}

/* ─── Status config ─── */

const STATUS_CONFIG: Record<
  string,
  { gradient: string; lozBg: string; lozText: string; icon: string; glow: string }
> = {
  Backlog: {
    gradient: "from-neutral-300 to-neutral-400 dark:from-neutral-600 dark:to-neutral-700",
    lozBg: "bg-neutral-100 dark:bg-neutral-800",
    lozText: "text-neutral-600 dark:text-neutral-300",
    icon: "M8 4v8m-4-4h8",
    glow: "shadow-neutral-300/30 dark:shadow-neutral-600/20",
  },
  Ready: {
    gradient: "from-blue-400 to-blue-500 dark:from-blue-500 dark:to-blue-600",
    lozBg: "bg-blue-50 dark:bg-blue-950/60",
    lozText: "text-blue-700 dark:text-blue-300",
    icon: "M5 8.5L8 11.5L13 5",
    glow: "shadow-blue-400/30 dark:shadow-blue-500/20",
  },
  "In Progress": {
    gradient: "from-blue-500 to-indigo-600 dark:from-blue-500 dark:to-indigo-500",
    lozBg: "bg-blue-600",
    lozText: "text-white",
    icon: "M12 8a4 4 0 11-8 0 4 4 0 018 0z",
    glow: "shadow-blue-500/40 dark:shadow-blue-500/30",
  },
  "In Review": {
    gradient: "from-amber-400 to-orange-500 dark:from-amber-500 dark:to-orange-500",
    lozBg: "bg-amber-100 dark:bg-amber-900/50",
    lozText: "text-amber-800 dark:text-amber-200",
    icon: "M8 4v4l3 2",
    glow: "shadow-amber-400/30 dark:shadow-amber-500/20",
  },
  Done: {
    gradient: "from-emerald-400 to-green-500 dark:from-emerald-500 dark:to-green-600",
    lozBg: "bg-green-100 dark:bg-green-900/50",
    lozText: "text-green-700 dark:text-green-300",
    icon: "M4 8.5L7 11.5L12 5",
    glow: "shadow-green-400/30 dark:shadow-green-500/20",
  },
  Blocked: {
    gradient: "from-red-400 to-rose-500 dark:from-red-500 dark:to-rose-500",
    lozBg: "bg-red-100 dark:bg-red-900/50",
    lozText: "text-red-700 dark:text-red-300",
    icon: "M5 5l6 6m0-6L5 11",
    glow: "shadow-red-400/30 dark:shadow-red-500/20",
  },
};

/* ─── Tag colors (Trello-style) ─── */

const TAG_PALETTE = [
  { bg: "#61BD4F", text: "#fff" },    // green
  { bg: "#F2D600", text: "#333" },    // yellow
  { bg: "#FF9F1A", text: "#fff" },    // orange
  { bg: "#EB5A46", text: "#fff" },    // red
  { bg: "#C377E0", text: "#fff" },    // purple
  { bg: "#0079BF", text: "#fff" },    // blue
  { bg: "#00C2E0", text: "#fff" },    // sky
  { bg: "#51E898", text: "#333" },    // lime
  { bg: "#FF78CB", text: "#fff" },    // pink
  { bg: "#344563", text: "#fff" },    // dark
];

/* ─── Helpers ─── */

function hash(s: string): number {
  let h = 0;
  for (let i = 0; i < s.length; i++) h = ((h << 5) - h + s.charCodeAt(i)) | 0;
  return Math.abs(h);
}

function formatRelative(iso: string): string {
  try {
    const d = new Date(iso);
    const now = new Date();
    const sec = Math.floor((now.getTime() - d.getTime()) / 1000);
    if (sec < 60) return "just now";
    if (sec < 3600) return `${Math.floor(sec / 60)}m ago`;
    if (sec < 86400) return `${Math.floor(sec / 3600)}h ago`;
    const days = Math.floor(sec / 86400);
    if (days < 7) return `${days}d ago`;
    return d.toLocaleDateString("en-US", { month: "short", day: "numeric", year: "numeric" });
  } catch {
    return iso;
  }
}

function initials(name: string): string {
  const parts = name.trim().split(/\s+/);
  if (parts.length >= 2) return (parts[0][0] + parts[1][0]).toUpperCase();
  return name.slice(0, 2).toUpperCase();
}

const AVATAR_GRADIENTS = [
  "linear-gradient(135deg, #667eea, #764ba2)",
  "linear-gradient(135deg, #f093fb, #f5576c)",
  "linear-gradient(135deg, #4facfe, #00f2fe)",
  "linear-gradient(135deg, #43e97b, #38f9d7)",
  "linear-gradient(135deg, #fa709a, #fee140)",
  "linear-gradient(135deg, #a18cd1, #fbc2eb)",
  "linear-gradient(135deg, #fccb90, #d57eeb)",
  "linear-gradient(135deg, #e0c3fc, #8ec5fc)",
];

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
        setTimeout(() => setOpen(false), 150);
      }
    };
    const handleEsc = (e: KeyboardEvent) => {
      if (e.key === "Escape") { setAnimating(false); setTimeout(() => setOpen(false), 150); }
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
    setTimeout(() => setOpen(false), 150);
  };

  const config = STATUS_CONFIG[status] || STATUS_CONFIG.Backlog;

  return (
    <div ref={ref} className="relative">
      <button
        onClick={open ? () => { setAnimating(false); setTimeout(() => setOpen(false), 150); } : handleOpen}
        className={cn(
          "inline-flex items-center gap-1.5 px-2.5 py-1 rounded text-[11px] font-bold uppercase tracking-wider",
          "transition-all duration-200 cursor-pointer select-none",
          config.lozBg, config.lozText,
          "hover:shadow-md active:scale-95"
        )}
      >
        <svg className="w-3.5 h-3.5" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
          <path d={config.icon} />
        </svg>
        {status}
        <svg
          className={cn("w-3 h-3 opacity-50 transition-transform duration-200", open && "rotate-180")}
          viewBox="0 0 12 12"
          fill="none"
        >
          <path d="M3 5L6 8L9 5" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
        </svg>
      </button>

      {open && (
        <div
          className={cn(
            "absolute top-full right-0 mt-2 z-50 min-w-[200px]",
            "bg-bg border border-border rounded-xl overflow-hidden",
            "transition-all duration-200 origin-top-right",
            animating
              ? "opacity-100 scale-100 shadow-2xl translate-y-0"
              : "opacity-0 scale-95 shadow-none -translate-y-1"
          )}
        >
          <div className="p-1">
            {STORY_STATUSES.map((s) => {
              const c = STATUS_CONFIG[s] || STATUS_CONFIG.Backlog;
              const isActive = s === status;
              return (
                <button
                  key={s}
                  onClick={() => handleSelect(s)}
                  className={cn(
                    "w-full flex items-center gap-3 px-3 py-2 rounded-lg text-xs text-left",
                    "transition-all duration-150",
                    isActive
                      ? "bg-blue-50 dark:bg-blue-950/40"
                      : "hover:bg-neutral-50 dark:hover:bg-neutral-800/60"
                  )}
                >
                  <div className={cn(
                    "w-3 h-3 rounded-full bg-gradient-to-br flex-shrink-0 transition-transform",
                    c.gradient,
                    isActive && "scale-125 ring-2 ring-blue-400/40"
                  )} />
                  <span className={cn("flex-1", isActive ? "font-semibold text-text" : "text-text/70")}>{s}</span>
                  {isActive && (
                    <svg className="w-4 h-4 text-blue-500" viewBox="0 0 16 16" fill="none">
                      <path d="M3 8.5L6.5 12L13 4" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
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

/* ─── Story Type Icon (Jira green story card) ─── */

function StoryTypeIcon() {
  return (
    <div
      className="w-5 h-5 rounded flex items-center justify-center flex-shrink-0"
      style={{ background: "linear-gradient(135deg, #36B37E, #00875A)" }}
      title="Story"
    >
      <svg className="w-3 h-3 text-white" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round">
        <rect x="2.5" y="2.5" width="11" height="11" rx="1.5" />
        <path d="M5 6h6M5 8.5h6M5 11h3" />
      </svg>
    </div>
  );
}

/* ─── Avatar ─── */

function Avatar({ name, size = 28 }: { name: string; size?: number }) {
  const gradient = AVATAR_GRADIENTS[hash(name) % AVATAR_GRADIENTS.length];
  return (
    <div
      className="rounded-full flex items-center justify-center font-bold text-white shadow-sm select-none"
      style={{
        width: size,
        height: size,
        background: gradient,
        fontSize: size * 0.38,
        letterSpacing: "0.02em",
      }}
    >
      {initials(name)}
    </div>
  );
}

/* ─── Points Ring ─── */

function PointsRing({ points }: { points: number }) {
  const circumference = 2 * Math.PI * 10;
  // Map points to a fill ratio (cap at 13 points)
  const ratio = Math.min(points / 13, 1);
  const offset = circumference * (1 - ratio);

  return (
    <div className="relative w-8 h-8 flex items-center justify-center" title={`${points} story points`}>
      <svg className="absolute inset-0 -rotate-90" viewBox="0 0 24 24">
        <circle cx="12" cy="12" r="10" fill="none" stroke="currentColor" className="text-border/40" strokeWidth="2" />
        <circle
          cx="12" cy="12" r="10" fill="none"
          stroke="url(#pointsGrad)"
          strokeWidth="2"
          strokeLinecap="round"
          strokeDasharray={circumference}
          strokeDashoffset={offset}
          className="transition-all duration-700 ease-out"
        />
        <defs>
          <linearGradient id="pointsGrad" x1="0" y1="0" x2="1" y2="1">
            <stop offset="0%" stopColor="#6366f1" />
            <stop offset="100%" stopColor="#8b5cf6" />
          </linearGradient>
        </defs>
      </svg>
      <span className="text-[11px] font-bold text-text/80 z-10">{points}</span>
    </div>
  );
}

/* ─── Tag Label ─── */

function TagLabel({ tag }: { tag: string }) {
  const color = TAG_PALETTE[hash(tag) % TAG_PALETTE.length];
  return (
    <span
      className="inline-flex items-center px-2 py-[3px] rounded text-[10px] font-bold uppercase tracking-wide transition-transform hover:scale-105"
      style={{
        backgroundColor: color.bg,
        color: color.text,
        textShadow: color.text === "#fff" ? "0 1px 2px rgba(0,0,0,0.15)" : "none",
      }}
    >
      {tag}
    </span>
  );
}

/* ─── Detail Row ─── */

function DetailRow({
  icon,
  label,
  children,
}: {
  icon: React.ReactNode;
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex items-center gap-3 py-1.5 group">
      <div className="flex items-center gap-2 w-24 flex-shrink-0">
        <span className="text-text/30 group-hover:text-text/50 transition-colors">{icon}</span>
        <span className="text-[11px] text-text/40 font-medium uppercase tracking-wider">{label}</span>
      </div>
      <div className="flex-1 text-xs text-text/80">{children}</div>
    </div>
  );
}

/* ─── Icons ─── */

const PersonIcon = (
  <svg className="w-3.5 h-3.5" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round">
    <circle cx="8" cy="5.5" r="2.5" />
    <path d="M3 14c0-2.8 2.2-4.5 5-4.5s5 1.7 5 4.5" />
  </svg>
);

const PointsIcon = (
  <svg className="w-3.5 h-3.5" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
    <polygon points="8,2 10,6.5 15,7 11.5,10.5 12.5,15 8,12.5 3.5,15 4.5,10.5 1,7 6,6.5" />
  </svg>
);

const ClockIcon = (
  <svg className="w-3.5 h-3.5" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round">
    <circle cx="8" cy="8" r="6" />
    <path d="M8 4.5V8l2.5 1.5" />
  </svg>
);

const CalendarIcon = (
  <svg className="w-3.5 h-3.5" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
    <rect x="2" y="3" width="12" height="11" rx="1.5" />
    <path d="M2 6.5h12M5.5 2v2M10.5 2v2" />
  </svg>
);

/* ─── Main Card ─── */

export function StoryMetaCard({ frontmatter, onChange }: StoryMetaCardProps) {
  const [isHovered, setIsHovered] = useState(false);

  const handleStatusChange = useCallback(
    (newStatus: string) => {
      onChange({
        ...frontmatter,
        status: newStatus,
        timestamps: {
          ...frontmatter.timestamps,
          updated_at: new Date().toISOString().replace(/\.\d{3}Z$/, "Z"),
        },
      });
    },
    [frontmatter, onChange]
  );

  const config = useMemo(
    () => STATUS_CONFIG[frontmatter.status] || STATUS_CONFIG.Backlog,
    [frontmatter.status]
  );

  return (
    <div
      className={cn(
        "mb-5 rounded-xl border border-border/60 bg-bg overflow-hidden",
        "transition-all duration-300 ease-out",
        isHovered ? `shadow-lg ${config.glow}` : "shadow-sm"
      )}
      onMouseEnter={() => setIsHovered(true)}
      onMouseLeave={() => setIsHovered(false)}
    >
      {/* Gradient status bar */}
      <div className={cn("h-1 bg-gradient-to-r transition-all duration-500", config.gradient)} />

      <div className="px-5 pt-4 pb-4">
        {/* Row 1: Type icon + Story key + Epic + Status lozenge */}
        <div className="flex items-center justify-between mb-2.5">
          <div className="flex items-center gap-2">
            <StoryTypeIcon />
            <span className="text-[13px] font-semibold text-blue-600 dark:text-blue-400 font-mono hover:underline cursor-default">
              {frontmatter.id}
            </span>
            <span className="text-[11px] text-text/25 font-mono">/</span>
            <span className="text-[11px] text-text/40 font-mono">{frontmatter.epic}</span>
          </div>
          <StatusDropdown status={frontmatter.status} onChangeStatus={handleStatusChange} />
        </div>

        {/* Title */}
        <h2 className="text-[17px] font-semibold text-text leading-snug mb-3 tracking-[-0.01em]">
          {frontmatter.title}
        </h2>

        {/* Tags */}
        {frontmatter.tags && frontmatter.tags.length > 0 && (
          <div className="flex flex-wrap gap-1.5 mb-4">
            {frontmatter.tags.map((tag) => (
              <TagLabel key={tag} tag={tag} />
            ))}
          </div>
        )}

        {/* Divider */}
        <div className="h-px bg-gradient-to-r from-border/60 via-border/20 to-transparent mb-3" />

        {/* Details */}
        <div>
          <DetailRow icon={PersonIcon} label="Assignee">
            {frontmatter.owner ? (
              <div className="flex items-center gap-2">
                <Avatar name={frontmatter.owner} size={22} />
                <span className="font-medium">{frontmatter.owner}</span>
              </div>
            ) : (
              <span className="text-text/30 italic">Unassigned</span>
            )}
          </DetailRow>

          {frontmatter.estimate_points !== undefined && (
            <DetailRow icon={PointsIcon} label="Points">
              <div className="flex items-center gap-2">
                <PointsRing points={frontmatter.estimate_points} />
                <span className="text-text/50 text-[11px]">
                  {frontmatter.estimate_points <= 2 ? "Small" : frontmatter.estimate_points <= 5 ? "Medium" : "Large"}
                </span>
              </div>
            </DetailRow>
          )}

          <DetailRow icon={ClockIcon} label="Updated">
            {formatRelative(frontmatter.timestamps.updated_at)}
          </DetailRow>

          <DetailRow icon={CalendarIcon} label="Created">
            {formatRelative(frontmatter.timestamps.created_at)}
          </DetailRow>
        </div>
      </div>
    </div>
  );
}
