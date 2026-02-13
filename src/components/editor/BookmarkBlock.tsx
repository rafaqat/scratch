import { useState, useEffect, useCallback } from "react";
import { createReactBlockSpec } from "@blocknote/react";
import { invoke } from "@tauri-apps/api/core";

/**
 * URL metadata returned by the Tauri backend.
 */
interface UrlMetadata {
  title: string;
  description: string;
  image: string;
  favicon: string;
  domain: string;
}

/**
 * Bookmark block spec for BlockNote.
 *
 * Renders a URL preview card with favicon, title, description, and domain.
 * Fetches metadata via Tauri backend on first render, then caches in block props.
 * Click opens URL in default browser.
 */
export const BookmarkBlock = createReactBlockSpec(
  {
    type: "bookmark" as const,
    propSchema: {
      url: { default: "" },
      title: { default: "" },
      description: { default: "" },
      image: { default: "" },
      favicon: { default: "" },
      domain: { default: "" },
      fetched: { default: "false" },
    },
    content: "none",
  },
  {
    render: (props) => {
      const { url, title, description, image, favicon, domain, fetched } =
        props.block.props;
      const [editing, setEditing] = useState(!url);
      const [localUrl, setLocalUrl] = useState(url);
      const [loading, setLoading] = useState(false);
      const [error, setError] = useState("");

      // Sync localUrl when url prop changes externally
      useEffect(() => {
        setLocalUrl(url);
      }, [url]);

      // Fetch metadata when URL is set but not yet fetched
      useEffect(() => {
        if (url && fetched !== "true") {
          fetchMetadata(url);
        }
        // eslint-disable-next-line react-hooks/exhaustive-deps
      }, [url, fetched]);

      const fetchMetadata = useCallback(
        async (targetUrl: string) => {
          setLoading(true);
          setError("");
          try {
            const meta = await invoke<UrlMetadata>("fetch_url_metadata", {
              url: targetUrl,
            });
            props.editor.updateBlock(props.block, {
              props: {
                url: targetUrl,
                title: meta.title || "",
                description: meta.description || "",
                image: meta.image || "",
                favicon: meta.favicon || "",
                domain: meta.domain || "",
                fetched: "true",
              },
            });
          } catch (err) {
            // Graceful fallback: show URL only
            const parsed = safeParseUrl(targetUrl);
            props.editor.updateBlock(props.block, {
              props: {
                url: targetUrl,
                title: "",
                description: "",
                image: "",
                favicon: "",
                domain: parsed?.hostname || "",
                fetched: "true",
              },
            });
            setError(
              err instanceof Error ? err.message : "Failed to fetch metadata"
            );
          } finally {
            setLoading(false);
          }
        },
        [props.editor, props.block]
      );

      const handleSubmit = useCallback(() => {
        const trimmed = localUrl.trim();
        if (!trimmed) return;

        // Add https:// if no protocol
        const finalUrl =
          trimmed.startsWith("http://") || trimmed.startsWith("https://")
            ? trimmed
            : `https://${trimmed}`;

        setLocalUrl(finalUrl);
        props.editor.updateBlock(props.block, {
          props: { url: finalUrl, fetched: "false" },
        });
        setEditing(false);
      }, [localUrl, props.editor, props.block]);

      const handleKeyDown = useCallback(
        (e: React.KeyboardEvent) => {
          if (e.key === "Enter") {
            e.preventDefault();
            handleSubmit();
          }
          if (e.key === "Escape") {
            e.preventDefault();
            if (url) {
              setLocalUrl(url);
              setEditing(false);
            }
          }
        },
        [handleSubmit, url]
      );

      const handleClick = useCallback(
        async (e: React.MouseEvent) => {
          e.preventDefault();
          e.stopPropagation();
          if (url) {
            try {
              await invoke("open_url_safe", { url });
            } catch (err) {
              console.error("Failed to open URL:", err);
            }
          }
        },
        [url]
      );

      // Edit mode: URL input
      if (editing) {
        return (
          <div className="bookmark-block bookmark-editing" contentEditable={false}>
            <div className="bookmark-input-row">
              <input
                className="bookmark-url-input"
                type="text"
                value={localUrl}
                onChange={(e) => setLocalUrl(e.target.value)}
                onKeyDown={handleKeyDown}
                onBlur={() => {
                  if (localUrl.trim()) handleSubmit();
                }}
                placeholder="Enter URL..."
                autoFocus
              />
            </div>
            <div className="bookmark-hint">
              Press Enter to embed link
            </div>
          </div>
        );
      }

      // Loading state
      if (loading) {
        return (
          <div className="bookmark-block bookmark-loading" contentEditable={false}>
            <div className="bookmark-spinner" />
            <span className="bookmark-loading-text">Fetching link preview...</span>
          </div>
        );
      }

      // Display mode: URL preview card
      const displayTitle = title || url;
      const displayDomain = domain || safeParseUrl(url)?.hostname || url;

      return (
        <div
          className="bookmark-block bookmark-card"
          contentEditable={false}
          onClick={handleClick}
          onDoubleClick={(e) => {
            e.preventDefault();
            e.stopPropagation();
            setEditing(true);
          }}
          title={`Open ${url} in browser (double-click to edit)`}
        >
          <div className="bookmark-card-content">
            <div className="bookmark-card-text">
              <div className="bookmark-card-title">{displayTitle}</div>
              {description && (
                <div className="bookmark-card-description">{description}</div>
              )}
              <div className="bookmark-card-meta">
                {favicon && (
                  <img
                    className="bookmark-card-favicon"
                    src={favicon}
                    alt=""
                    width={16}
                    height={16}
                    onError={(e) => {
                      (e.target as HTMLImageElement).style.display = "none";
                    }}
                  />
                )}
                <span className="bookmark-card-domain">{displayDomain}</span>
              </div>
            </div>
            {image && (
              <div className="bookmark-card-image">
                <img
                  src={image}
                  alt=""
                  onError={(e) => {
                    (e.target as HTMLImageElement).style.display = "none";
                  }}
                />
              </div>
            )}
          </div>
          {error && (
            <div className="bookmark-card-error">{error}</div>
          )}
        </div>
      );
    },

    toExternalHTML: (props) => {
      const { url, title, description, favicon, domain } = props.block.props;
      // Serialize as a data-annotated div that our postprocessor can convert
      // back to the markdown bookmark syntax
      return (
        <div
          data-bookmark-block="true"
          data-bookmark-url={url}
          data-bookmark-title={title}
          data-bookmark-description={description}
          data-bookmark-favicon={favicon}
          data-bookmark-domain={domain}
        >
          {url}
        </div>
      );
    },

    parse: (element: HTMLElement) => {
      if (element.getAttribute("data-bookmark-block") === "true") {
        return {
          url: element.getAttribute("data-bookmark-url") || "",
          title: element.getAttribute("data-bookmark-title") || "",
          description: element.getAttribute("data-bookmark-description") || "",
          image: "",
          favicon: element.getAttribute("data-bookmark-favicon") || "",
          domain: element.getAttribute("data-bookmark-domain") || "",
          fetched: "true",
        };
      }
      return undefined;
    },
  }
);

/**
 * Safely parse a URL, returning null on failure.
 */
function safeParseUrl(url: string): URL | null {
  try {
    return new URL(url);
  } catch {
    return null;
  }
}
