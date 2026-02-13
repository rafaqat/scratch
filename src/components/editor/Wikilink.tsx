import { createReactInlineContentSpec } from "@blocknote/react";
import type { BlockNoteEditor } from "@blocknote/core";
import type { NoteMetadata } from "../../types/note";

/**
 * Global set of note titles (lowercase) for broken link detection.
 * Updated by Editor.tsx whenever the notes list changes.
 */
export const noteTitlesSet = new Set<string>();

export function updateNoteTitles(notes: NoteMetadata[]) {
  noteTitlesSet.clear();
  for (const note of notes) {
    noteTitlesSet.add(note.title.toLowerCase());
  }
}

/**
 * Wikilink inline content spec for BlockNote.
 *
 * Renders [[Note Title]] as a styled pill in the editor.
 * Atomic (content: "none") — the user can't edit inside it,
 * only delete the whole node or click to navigate.
 */
export const Wikilink = createReactInlineContentSpec(
  {
    type: "wikilink" as const,
    propSchema: {
      title: { default: "" },
      alias: { default: "" },
    },
    content: "none",
  },
  {
    render: (props) => {
      const { title, alias } = props.inlineContent.props;
      const displayText = alias || title;
      const isBroken = title && !noteTitlesSet.has(title.toLowerCase());

      const handleClick = (e: React.MouseEvent) => {
        e.preventDefault();
        e.stopPropagation();
        window.dispatchEvent(
          new CustomEvent("wikilink-navigate", { detail: { title } }),
        );
      };

      return (
        <span
          className={`wikilink${isBroken ? " wikilink-broken" : ""}`}
          data-wikilink-title={title}
          onClick={handleClick}
          contentEditable={false}
          title={isBroken ? `"${title}" not found` : alias ? `→ ${title}` : undefined}
        >
          {displayText}
        </span>
      );
    },

    // External HTML representation — used by blocksToMarkdownLossy.
    // Preserves alias syntax: [[Title|alias]] or [[Title]]
    toExternalHTML: (props) => {
      const { title, alias } = props.inlineContent.props;
      const text = alias ? `${title}|${alias}` : title;
      return <span>[[{text}]]</span>;
    },
  },
);

/**
 * Build suggestion menu items from the notes list.
 * Filters by query (typed after [[) and returns items
 * that insert a wikilink node when clicked.
 */
export function getWikilinkMenuItems(
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  editor: BlockNoteEditor<any, any, any>,
  notes: NoteMetadata[],
  query: string,
) {
  const lowerQuery = query.toLowerCase();
  const filtered = notes.filter((note) =>
    note.title.toLowerCase().includes(lowerQuery),
  );

  return filtered.slice(0, 10).map((note) => ({
    title: note.title,
    onItemClick: () => {
      editor.insertInlineContent([
        {
          type: "wikilink" as const,
          props: { title: note.title, alias: "" },
        },
        " ",
      ]);
    },
  }));
}
