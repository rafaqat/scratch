import { useState, useMemo, useRef, useEffect } from "react";

/**
 * Lightweight emoji picker with categories and search.
 * No external dependencies — uses a curated set of common emojis.
 */

const EMOJI_CATEGORIES: Record<string, string[]> = {
  "Smileys": ["😀", "😃", "😄", "😁", "😅", "😂", "🤣", "😊", "😇", "🙂", "😉", "😍", "🥰", "😘", "😎", "🤩", "🥳", "😏", "🤔", "🤫", "🤭", "😐", "😑", "😶", "🙄", "😬", "😮‍💨", "🤥", "😌", "😔", "😴", "🤤", "😷", "🤒", "🤕"],
  "People": ["👋", "🤚", "✋", "🖖", "👌", "🤌", "✌️", "🤞", "🫰", "🤟", "🤙", "👍", "👎", "✊", "👊", "🤛", "🤜", "👏", "🙌", "🫶", "👐", "🤝", "🙏", "✍️", "💪", "🦾", "🧠", "👀", "👁️", "👅", "👄"],
  "Objects": ["📝", "📒", "📕", "📗", "📘", "📙", "📓", "📔", "📚", "📖", "🔖", "📎", "📌", "📐", "📏", "✏️", "🖊️", "🖋️", "✒️", "🔍", "🔎", "💡", "🔦", "🏮", "📁", "📂", "🗂️", "📋", "📄", "📃", "📑", "🗒️", "🗓️"],
  "Symbols": ["⭐", "🌟", "✨", "💫", "🔥", "❤️", "🧡", "💛", "💚", "💙", "💜", "🖤", "🤍", "🤎", "💔", "❤️‍🔥", "💯", "💢", "💥", "💫", "💦", "🔴", "🟠", "🟡", "🟢", "🔵", "🟣", "⚫", "⚪", "🟤", "✅", "❌", "⚡", "♻️"],
  "Nature": ["🌱", "🌲", "🌳", "🌴", "🌵", "🌾", "🌿", "☘️", "🍀", "🍁", "🍂", "🍃", "🌸", "🌺", "🌻", "🌹", "🌷", "💐", "🍄", "🐱", "🐶", "🐻", "🐼", "🦁", "🐸", "🦋", "🐝", "🐞", "🕊️"],
  "Food": ["🍎", "🍊", "🍋", "🍌", "🍉", "🍇", "🍓", "🫐", "🍑", "🥭", "🍒", "🥝", "🍅", "🥑", "🥦", "🧁", "🍰", "🎂", "🍪", "🍩", "🍫", "☕", "🍵", "🧋", "🥤", "🍺", "🍷", "🥂", "🍾"],
  "Travel": ["🏠", "🏡", "🏢", "🏣", "🏥", "🏦", "🏛️", "⛪", "🕌", "🕍", "🗼", "🗽", "🏗️", "🌁", "🌃", "🌄", "🌅", "🌆", "🌇", "🚀", "✈️", "🚁", "🛩️", "🚂", "🚗", "🚕", "🏎️"],
  "Activities": ["⚽", "🏀", "🏈", "⚾", "🥎", "🎾", "🏐", "🏉", "🥏", "🎱", "🏓", "🏸", "🏒", "⛳", "🎯", "🎮", "🕹️", "🎲", "🧩", "🎨", "🎭", "🎪", "🎬", "🎤", "🎧", "🎵", "🎶", "🎹", "🥁", "🎸"],
  "Tech": ["💻", "🖥️", "🖨️", "⌨️", "🖱️", "🖲️", "💾", "💿", "📱", "📞", "☎️", "📟", "📠", "🔋", "🔌", "📡", "🛰️", "🤖", "🦾", "⚙️", "🔧", "🔨", "🛠️", "⛏️", "🔩", "🧲"],
};

const ALL_EMOJIS = Object.values(EMOJI_CATEGORIES).flat();

// Random suggestion from common "note" emojis
const SUGGESTION_POOL = ["📝", "📒", "📌", "💡", "⭐", "🔥", "🎯", "🚀", "💎", "🌟", "📚", "🧩", "🎨", "✨", "🌱", "🔖", "🏷️", "📋"];

export function getRandomEmoji(): string {
  return SUGGESTION_POOL[Math.floor(Math.random() * SUGGESTION_POOL.length)];
}

interface EmojiPickerProps {
  onSelect: (emoji: string) => void;
  onClose: () => void;
}

export function EmojiPicker({ onSelect, onClose }: EmojiPickerProps) {
  const [search, setSearch] = useState("");
  const [activeCategory, setActiveCategory] = useState("Smileys");
  const inputRef = useRef<HTMLInputElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  // Close on click outside
  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (containerRef.current && !containerRef.current.contains(e.target as Node)) {
        onClose();
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [onClose]);

  // Close on Escape
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [onClose]);

  const filteredEmojis = useMemo(() => {
    if (!search) return null;
    // Simple search: filter all emojis (emoji characters don't have text names, so we filter categories)
    const q = search.toLowerCase();
    const results: string[] = [];
    for (const [cat, emojis] of Object.entries(EMOJI_CATEGORIES)) {
      if (cat.toLowerCase().includes(q)) {
        results.push(...emojis);
      }
    }
    // Also include any emoji that matches via text representation
    if (results.length === 0) {
      return ALL_EMOJIS.slice(0, 40); // fallback
    }
    return results;
  }, [search]);

  const categories = Object.keys(EMOJI_CATEGORIES);

  return (
    <div
      ref={containerRef}
      className="emoji-picker"
      onClick={(e) => e.stopPropagation()}
    >
      <input
        ref={inputRef}
        type="text"
        placeholder="Search emojis..."
        value={search}
        onChange={(e) => setSearch(e.target.value)}
        className="emoji-picker-search"
      />
      {!search && (
        <div className="emoji-picker-categories">
          {categories.map((cat) => (
            <button
              key={cat}
              className={`emoji-picker-cat-btn ${activeCategory === cat ? "active" : ""}`}
              onClick={() => setActiveCategory(cat)}
            >
              {cat}
            </button>
          ))}
        </div>
      )}
      <div className="emoji-picker-grid">
        {(filteredEmojis || EMOJI_CATEGORIES[activeCategory] || []).map((emoji, i) => (
          <button
            key={`${emoji}-${i}`}
            className="emoji-picker-item"
            onClick={() => onSelect(emoji)}
          >
            {emoji}
          </button>
        ))}
      </div>
    </div>
  );
}
