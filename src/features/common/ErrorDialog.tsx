import type { CommandError } from "../../lib/bookTypes";
import { errorHeading } from "../../lib/errors";
import { Modal } from "./Modal";

interface ErrorDialogProps {
  error: CommandError;
  onClose: () => void;
}

/** Shows a failed command's heading and message, with one OK button. */
export function ErrorDialog({ error, onClose }: ErrorDialogProps) {
  return (
    <Modal title={errorHeading(error.kind)} onClose={onClose}>
      <p className="modal-message" role="alert">
        {error.message}
      </p>
      <div className="modal-actions">
        <button type="button" className="button primary" onClick={onClose}>
          OK
        </button>
      </div>
    </Modal>
  );
}
