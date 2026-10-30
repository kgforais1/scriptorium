import { useEffect, useId, useRef, type ReactNode } from "react";
import "./common.css";

interface ModalProps {
  /** Heading text. It also names the dialog for assistive technology. */
  title: string;
  /** Called on Escape. Null means the dialog cannot be dismissed right now. */
  onClose: (() => void) | null;
  children: ReactNode;
}

/**
 * A modal dialog built on the native `<dialog>` element. The browser traps focus inside it
 * and makes the rest of the page inert. Escape calls `onClose`, and focus returns to the
 * control that had it before the dialog opened.
 */
export function Modal({ title, onClose, children }: Readonly<ModalProps>) {
  const ref = useRef<HTMLDialogElement>(null);
  const headingId = useId();

  useEffect(() => {
    const dialog = ref.current;
    if (!dialog) {
      return;
    }
    const previous = document.activeElement;
    if (!dialog.open) {
      dialog.showModal();
    }
    return () => {
      dialog.close();
      // The opener may be gone, as when a new screen replaced it.
      if (previous instanceof HTMLElement && previous.isConnected) {
        previous.focus();
      }
    };
  }, []);

  return (
    <dialog
      ref={ref}
      className="modal"
      aria-labelledby={headingId}
      onCancel={(event) => {
        // React owns whether the dialog exists, so the browser must not close it itself.
        event.preventDefault();
        onClose?.();
      }}
    >
      <h2 id={headingId} className="modal-title">
        {title}
      </h2>
      {children}
    </dialog>
  );
}
