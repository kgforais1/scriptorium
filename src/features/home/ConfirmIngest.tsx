import { useEffect, useRef, useState } from "react";
import { createBook, onIngestProgress } from "../../lib/api";
import { bookIdHint } from "../../lib/bookId";
import type { BookView, CommandError, ScansFolderInfo } from "../../lib/bookTypes";
import { toCommandError } from "../../lib/errors";
import { formatMegabytes, progressLabel } from "../../lib/format";
import { Modal } from "../common/Modal";
import "./home.css";

interface ConfirmIngestProps {
  /** The folder of scans that was picked. */
  folder: string;
  info: ScansFolderInfo;
  /** Called with the new book once it is created. */
  onCreated: (book: BookView) => void;
  onCancel: () => void;
  onError: (error: CommandError) => void;
}

interface TextFieldProps {
  label: string;
  value: string;
  onChange: (value: string) => void;
  invalid?: boolean;
  describedBy?: string;
}

function TextField({ label, value, onChange, invalid, describedBy }: TextFieldProps) {
  return (
    <label className="field">
      <span>{label}</span>
      <input
        type="text"
        value={value}
        aria-invalid={invalid ? "true" : undefined}
        aria-describedby={describedBy}
        autoComplete="off"
        spellCheck={false}
        onChange={(event) => onChange(event.target.value)}
      />
    </label>
  );
}

/**
 * Asks the user to confirm turning a folder of scans into a book. It shows the counts and
 * estimated size, lets them edit the metadata, and shows progress while the book is built.
 */
export function ConfirmIngest({ folder, info, onCreated, onCancel, onError }: ConfirmIngestProps) {
  const [title, setTitle] = useState(info.suggestedTitle);
  const [id, setId] = useState(info.suggestedId);
  const [rights, setRights] = useState("");
  const [attribution, setAttribution] = useState("");
  const [progress, setProgress] = useState<number | null>(null);

  const hint = bookIdHint(id);
  const titleMissing = title.trim() === "";
  const canCreate = progress === null && hint === null && !titleMissing;
  const ingesting = progress !== null;
  // Every control is disabled while the book is built, so focus moves to the progress text.
  const progressRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (ingesting) {
      progressRef.current?.focus();
    }
  }, [ingesting]);

  async function create() {
    setProgress(0);
    let unlisten: (() => void) | undefined;
    try {
      unlisten = await onIngestProgress((tick) => setProgress(tick.done));
      onCreated(await createBook({ folder, title: title.trim(), id, rights, attribution }));
    } catch (err) {
      setProgress(null);
      onError(toCommandError(err));
    } finally {
      unlisten?.();
    }
  }

  return (
    <Modal title="Create a book from these scans?" onClose={progress === null ? onCancel : null}>
      <p className="ingest-summary">
        {info.files} image files, {info.frames} pages, about {formatMegabytes(info.estimatedBytes)}.
      </p>
      <form
        aria-busy={ingesting}
        onSubmit={(event) => {
          event.preventDefault();
          if (canCreate) {
            void create();
          }
        }}
      >
        <fieldset className="ingest-form" disabled={progress !== null}>
          <TextField
            label="Title"
            value={title}
            onChange={setTitle}
            invalid={titleMissing}
            describedBy={titleMissing ? "title-hint" : undefined}
          />
          {titleMissing && (
            <p id="title-hint" className="field-hint">
              Enter a title.
            </p>
          )}
          <TextField
            label="Book id"
            value={id}
            onChange={setId}
            invalid={hint !== null}
            describedBy={hint === null ? undefined : "book-id-hint"}
          />
          {hint !== null && (
            <p id="book-id-hint" className="field-hint">
              {hint}
            </p>
          )}
          <TextField label="Rights (optional)" value={rights} onChange={setRights} />
          <TextField label="Attribution (optional)" value={attribution} onChange={setAttribution} />
        </fieldset>
        {progress !== null && (
          <div className="ingest-progress" ref={progressRef} tabIndex={-1}>
            <progress max={info.frames} value={progress} aria-label="Import progress" />
            <span role="status">{progressLabel(progress, info.frames)}</span>
          </div>
        )}
        <div className="modal-actions">
          <button type="button" className="button" onClick={onCancel} disabled={progress !== null}>
            Cancel
          </button>
          <button type="submit" className="button primary" disabled={!canCreate}>
            Create
          </button>
        </div>
      </form>
    </Modal>
  );
}
