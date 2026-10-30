import { useEffect, useRef, useState } from "react";
import { createBook, onIngestProgress } from "../../lib/api";
import { bookIdHint } from "../../lib/bookId";
import type { BookView, CommandError, ScansFolderInfo } from "../../lib/bookTypes";
import { toCommandError } from "../../lib/errors";
import { formatMegabytes, progressLabel } from "../../lib/format";
import { lengthHint, TEXT_FIELD_MAX_LEN, TITLE_MAX_LEN } from "../../lib/limits";
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
  /** A problem with the value, shown under the field. Null when the value is fine. */
  hint: string | null;
  /** Element id for the hint, so the input can point at it. */
  hintId: string;
}

function TextField({ label, value, onChange, hint, hintId }: Readonly<TextFieldProps>) {
  return (
    <>
      <label className="field">
        <span>{label}</span>
        <input
          type="text"
          value={value}
          aria-invalid={hint === null ? undefined : "true"}
          aria-describedby={hint === null ? undefined : hintId}
          autoComplete="off"
          spellCheck={false}
          onChange={(event) => onChange(event.target.value)}
        />
      </label>
      {hint !== null && (
        <p id={hintId} className="field-hint">
          {hint}
        </p>
      )}
    </>
  );
}

/**
 * Asks the user to confirm turning a folder of scans into a book. It shows the counts and
 * estimated size, lets them edit the metadata, and shows progress while the book is built.
 */
export function ConfirmIngest({ folder, info, onCreated, onCancel, onError }: Readonly<ConfirmIngestProps>) {
  const [title, setTitle] = useState(info.suggestedTitle);
  const [id, setId] = useState(info.suggestedId);
  const [rights, setRights] = useState("");
  const [attribution, setAttribution] = useState("");
  const [progress, setProgress] = useState<number | null>(null);

  // Every limit `meta.json` enforces is checked here, before any scan is copied.
  const idHint = bookIdHint(id);
  const titleHint =
    title.trim() === "" ? "Enter a title." : lengthHint(title.trim(), TITLE_MAX_LEN);
  const rightsHint = lengthHint(rights, TEXT_FIELD_MAX_LEN);
  const attributionHint = lengthHint(attribution, TEXT_FIELD_MAX_LEN);
  const canCreate =
    progress === null && [idHint, titleHint, rightsHint, attributionHint].every((h) => h === null);
  const ingesting = progress !== null;
  // Every control is disabled while the book is built, so focus moves to the progress text.
  // If the build fails, focus returns to Create, so it is not lost on the page body.
  const progressRef = useRef<HTMLDivElement>(null);
  const createRef = useRef<HTMLButtonElement>(null);
  const wasIngesting = useRef(false);
  useEffect(() => {
    if (ingesting) {
      progressRef.current?.focus();
    } else if (wasIngesting.current) {
      createRef.current?.focus();
    }
    wasIngesting.current = ingesting;
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
            hint={titleHint}
            hintId="title-hint"
          />
          <TextField
            label="Book id"
            value={id}
            onChange={setId}
            hint={idHint}
            hintId="book-id-hint"
          />
          <TextField
            label="Rights (optional)"
            value={rights}
            onChange={setRights}
            hint={rightsHint}
            hintId="rights-hint"
          />
          <TextField
            label="Attribution (optional)"
            value={attribution}
            onChange={setAttribution}
            hint={attributionHint}
            hintId="attribution-hint"
          />
        </fieldset>
        {progress !== null && (
          <div className="ingest-progress" ref={progressRef} tabIndex={-1}>
            <progress max={info.frames} value={progress} aria-label="Import progress" />
            <output>{progressLabel(progress, info.frames)}</output>
          </div>
        )}
        <div className="modal-actions">
          <button type="button" className="button" onClick={onCancel} disabled={progress !== null}>
            Cancel
          </button>
          <button
            type="submit"
            className="button primary"
            disabled={!canCreate}
            ref={createRef}
          >
            Create
          </button>
        </div>
      </form>
    </Modal>
  );
}
