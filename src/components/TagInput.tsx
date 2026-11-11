import { useState, type KeyboardEvent } from "react";
import { Icon } from "./Icon";

const MAX_TAG_LEN = 32;
const MAX_TAGS = 16;

/** Same normalization as tnls_core::tunnel::normalize_tags. */
function addTag(tags: string[], raw: string): string[] {
  const tag = raw.trim().slice(0, MAX_TAG_LEN);
  if (tag === "" || tags.length >= MAX_TAGS) return tags;
  if (tags.some((t) => t.toLowerCase() === tag.toLowerCase())) return tags;
  return [...tags, tag];
}

/**
 * Chip editor for tunnel tags: Enter or comma adds, Backspace on an empty
 * input removes the last chip. Existing tags from other tunnels are offered
 * as suggestions so spellings stay consistent.
 */
export function TagInput({
  id,
  tags,
  suggestions,
  onChange,
}: {
  id: string;
  tags: string[];
  suggestions: string[];
  onChange: (tags: string[]) => void;
}) {
  const [draft, setDraft] = useState("");
  const listId = `${id}-suggestions`;
  const unused = suggestions.filter(
    (s) => !tags.some((t) => t.toLowerCase() === s.toLowerCase()),
  );

  const commit = () => {
    if (draft.trim() === "") return;
    onChange(addTag(tags, draft));
    setDraft("");
  };

  const onKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter" || e.key === ",") {
      e.preventDefault();
      commit();
    } else if (e.key === "Backspace" && draft === "" && tags.length > 0) {
      onChange(tags.slice(0, -1));
    }
  };

  return (
    <div className="tag-input">
      {tags.map((tag) => (
        <span key={tag} className="tag-chip tag-chip--removable">
          {tag}
          <button
            type="button"
            aria-label={`Remove tag ${tag}`}
            onClick={() => onChange(tags.filter((t) => t !== tag))}
          >
            <Icon name="x" size={10} />
          </button>
        </span>
      ))}
      <input
        id={id}
        className="tag-input__field"
        type="text"
        autoComplete="off"
        spellCheck={false}
        list={listId}
        maxLength={MAX_TAG_LEN}
        placeholder={tags.length === 0 ? "homelab, 9router…" : ""}
        value={draft}
        onChange={(e) => {
          const v = e.target.value;
          // Picking a datalist suggestion fires a change with the full value.
          if (unused.includes(v)) {
            onChange(addTag(tags, v));
            setDraft("");
          } else {
            setDraft(v);
          }
        }}
        onKeyDown={onKeyDown}
        onBlur={commit}
      />
      <datalist id={listId}>
        {unused.map((s) => (
          <option key={s} value={s} />
        ))}
      </datalist>
    </div>
  );
}
